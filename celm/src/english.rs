//! Data-driven English generation from WordNet and the tagged Brown corpus.

use super::{
    BigNat, ChoiceState, Error, GRAMMAR_VERSION, LEXICON_VERSION, SPEC_VERSION, TraceEvent,
    recover_from_trace,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const ENGLISH_PROFILE_VERSION: &str = "wordnet-3.1+brown-1979-pattern-v0.2";
const START: &str = "<START>";
const END: &str = "<END>";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaggedWord {
    pub tag: String,
    pub word: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnglishMeaning {
    pub words: Vec<TaggedWord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnglishOutput {
    pub sentence: String,
    pub meaning: EnglishMeaning,
    pub requested_words: usize,
    pub declared_bits: u32,
    pub residual: BigNat,
    pub trace: Vec<TraceEvent>,
    pub profile_version: &'static str,
    pub spec_version: &'static str,
    pub grammar_version: &'static str,
    pub lexicon_version: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnglishProfileStats {
    pub wordnet_nouns: usize,
    pub wordnet_verbs: usize,
    pub wordnet_adjectives: usize,
    pub wordnet_adverbs: usize,
    pub corpus_sentences: usize,
    pub corpus_tokens: usize,
    pub tags: usize,
    pub lexical_forms: usize,
    pub transitions: usize,
}

#[derive(Clone, Copy, Debug)]
struct Transition {
    to: usize,
    weight: u32,
}

#[derive(Clone, Debug)]
struct LexicalForm {
    word: String,
    weight: u32,
}

#[derive(Clone, Debug)]
struct PatternToken {
    tag_id: usize,
    corpus_word: String,
}

/// A compiled probabilistic language profile. Candidate tables are sorted, so
/// identical datasets and numeric input produce identical output everywhere.
pub struct EnglishProfile {
    tags: Vec<String>,
    words: Vec<Vec<LexicalForm>>,
    transitions: Vec<Vec<Transition>>,
    patterns: BTreeMap<usize, Vec<Vec<PatternToken>>>,
    start: usize,
    end: usize,
    stats: EnglishProfileStats,
}

impl EnglishProfile {
    pub fn load_default() -> Result<Self, Error> {
        let root = std::env::var_os("CELM_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data"));
        Self::load(root)
    }

    pub fn load(root: impl AsRef<Path>) -> Result<Self, Error> {
        let root = root.as_ref();
        let wordnet = root.join("wordnet/dict");
        let brown = root.join("brown");

        let nouns = load_wordnet_index(&wordnet.join("index.noun"))?;
        let verbs = load_wordnet_index(&wordnet.join("index.verb"))?;
        let adjectives = load_wordnet_index(&wordnet.join("index.adj"))?;
        let adverbs = load_wordnet_index(&wordnet.join("index.adv"))?;

        let mut tag_words: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
        let mut transition_counts: BTreeMap<(String, String), u32> = BTreeMap::new();
        let mut corpus_patterns = Vec::new();
        let mut corpus_sentences = 0usize;
        let mut corpus_tokens = 0usize;

        let mut files: Vec<_> = fs::read_dir(&brown)
            .map_err(|error| data_error(&brown, error))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_brown_text_file(path))
            .collect();
        files.sort();

        for path in files {
            let contents = fs::read_to_string(&path).map_err(|error| data_error(&path, error))?;
            for line in contents.lines() {
                let mut tagged = Vec::new();
                for raw in line.split_whitespace() {
                    let Some((word, tag)) = raw.rsplit_once('/') else {
                        continue;
                    };
                    let word = normalize_corpus_word(word);
                    if word.is_empty() {
                        continue;
                    }
                    let tag = normalize_tag(tag);
                    if tag.is_empty() || is_punctuation_tag(&tag) {
                        continue;
                    }
                    let weight = tag_words
                        .entry(tag.clone())
                        .or_default()
                        .entry(word.clone())
                        .or_default();
                    *weight = weight.saturating_add(16);
                    tagged.push((tag, word));
                }
                if tagged.is_empty() {
                    continue;
                }
                corpus_sentences += 1;
                corpus_tokens += tagged.len();
                corpus_patterns.push(tagged.clone());
                increment(&mut transition_counts, START, &tagged[0].0);
                for pair in tagged.windows(2) {
                    increment(&mut transition_counts, &pair[0].0, &pair[1].0);
                }
                increment(&mut transition_counts, &tagged.last().unwrap().0, END);
            }
        }

        // Base-form Brown tags are opened to all single-token WordNet lemmas.
        // Inflected forms and function words remain corpus-derived.
        extend_dictionary(tag_words.entry("NN".into()).or_default(), &nouns);
        extend_dictionary(tag_words.entry("VB".into()).or_default(), &verbs);
        extend_dictionary(tag_words.entry("JJ".into()).or_default(), &adjectives);
        extend_dictionary(tag_words.entry("RB".into()).or_default(), &adverbs);

        if corpus_sentences == 0 || tag_words.is_empty() {
            return Err(Error::Data(
                "Brown corpus contained no tagged sentences".into(),
            ));
        }

        let tags: Vec<_> = tag_words.keys().cloned().collect();
        let tag_ids: BTreeMap<_, _> = tags
            .iter()
            .enumerate()
            .map(|(index, tag)| (tag.clone(), index))
            .collect();
        let start = tags.len();
        let end = start + 1;
        let mut transitions = vec![Vec::new(); tags.len() + 2];
        for ((from, to), weight) in transition_counts {
            let from_id = if from == START {
                start
            } else {
                *tag_ids
                    .get(&from)
                    .ok_or_else(|| Error::Data(format!("unknown source tag {from}")))?
            };
            let to_id = if to == END {
                end
            } else {
                *tag_ids
                    .get(&to)
                    .ok_or_else(|| Error::Data(format!("unknown target tag {to}")))?
            };
            transitions[from_id].push(Transition { to: to_id, weight });
        }
        for candidates in &mut transitions {
            candidates.sort_by_key(|candidate| candidate.to);
        }

        let words: Vec<Vec<LexicalForm>> = tags
            .iter()
            .map(|tag| {
                tag_words
                    .remove(tag)
                    .unwrap()
                    .into_iter()
                    .map(|(word, weight)| LexicalForm { word, weight })
                    .collect()
            })
            .collect();
        let mut patterns: BTreeMap<usize, Vec<Vec<PatternToken>>> = BTreeMap::new();
        for pattern in corpus_patterns {
            let tokens: Vec<_> = pattern
                .iter()
                .map(|(tag, word)| {
                    let tag_id = tag_ids
                        .get(tag)
                        .copied()
                        .ok_or_else(|| Error::Data(format!("unknown pattern tag {tag}")))?;
                    Ok(PatternToken {
                        tag_id,
                        corpus_word: word.clone(),
                    })
                })
                .collect::<Result<_, _>>()?;
            patterns.entry(tokens.len()).or_default().push(tokens);
        }
        let lexical_forms = words.iter().map(Vec::len).sum();
        let transition_total = transitions.iter().map(Vec::len).sum();
        let stats = EnglishProfileStats {
            wordnet_nouns: nouns.len(),
            wordnet_verbs: verbs.len(),
            wordnet_adjectives: adjectives.len(),
            wordnet_adverbs: adverbs.len(),
            corpus_sentences,
            corpus_tokens,
            tags: tags.len(),
            lexical_forms,
            transitions: transition_total,
        };
        Ok(Self {
            tags,
            words,
            transitions,
            patterns,
            start,
            end,
            stats,
        })
    }

    pub fn stats(&self) -> EnglishProfileStats {
        self.stats
    }

    pub fn generate(
        &self,
        choice: &ChoiceState,
        requested_words: usize,
    ) -> Result<EnglishOutput, Error> {
        if !(super::MIN_GENERATED_WORDS..=super::MAX_GENERATED_WORDS).contains(&requested_words) {
            return Err(Error::NoRealization(
                "requested word count must be between 3 and 4096",
            ));
        }
        let mut residual = choice.integer.clone();
        let mut trace = Vec::with_capacity(requested_words * 2);
        let tag_sequence: Vec<(usize, Option<String>)> =
            if let Some(patterns) = self.patterns.get(&requested_words) {
                let before = residual.to_string();
                let index = residual.div_rem_small(patterns.len() as u32);
                trace.push(TraceEvent {
                    field: "english_pattern",
                    radix: patterns.len() as u32,
                    index,
                    candidate_id: index,
                    residual_before: before,
                    residual_after: residual.to_string(),
                });
                patterns[index as usize]
                    .iter()
                    .map(|token| (token.tag_id, Some(token.corpus_word.clone())))
                    .collect()
            } else {
                self.select_tag_sequence(requested_words, &mut residual, &mut trace)?
                    .into_iter()
                    .map(|tag_id| (tag_id, None))
                    .collect()
            };
        let mut selected = Vec::with_capacity(requested_words);

        for (tag_id, corpus_word) in tag_sequence {
            let word = match corpus_word {
                Some(word) if !dictionary_open_tag(&self.tags[tag_id]) => word,
                _ => select_word(&mut residual, &self.words[tag_id], &mut trace)?
                    .word
                    .clone(),
            };
            selected.push(TaggedWord {
                tag: self.tags[tag_id].clone(),
                word,
            });
        }

        let meaning = EnglishMeaning { words: selected };
        let output = EnglishOutput {
            sentence: render(&meaning),
            meaning,
            requested_words,
            declared_bits: choice.declared_bits,
            residual,
            trace,
            profile_version: ENGLISH_PROFILE_VERSION,
            spec_version: SPEC_VERSION,
            grammar_version: GRAMMAR_VERSION,
            lexicon_version: LEXICON_VERSION,
        };
        if !self.verify(&output) {
            return Err(Error::VerificationFailure);
        }
        Ok(output)
    }

    pub fn verify(&self, output: &EnglishOutput) -> bool {
        if output.profile_version != ENGLISH_PROFILE_VERSION
            || output.spec_version != SPEC_VERSION
            || output.grammar_version != GRAMMAR_VERSION
            || output.lexicon_version != LEXICON_VERSION
            || output.meaning.words.len() != output.requested_words
            || render(&output.meaning) != output.sentence
        {
            return false;
        }
        let mut current = self.start;
        for tagged in &output.meaning.words {
            let Ok(tag_id) = self.tags.binary_search(&tagged.tag) else {
                return false;
            };
            if self.words[tag_id]
                .binary_search_by(|form| form.word.as_str().cmp(&tagged.word))
                .is_err()
                || !self.transitions[current]
                    .iter()
                    .any(|transition| transition.to == tag_id)
            {
                return false;
            }
            current = tag_id;
        }
        self.transitions[current]
            .iter()
            .any(|transition| transition.to == self.end)
    }

    pub fn recover_integer(&self, output: &EnglishOutput) -> Result<BigNat, Error> {
        recover_from_trace(&output.residual, &output.trace)
    }

    fn select_tag_sequence(
        &self,
        requested_words: usize,
        residual: &mut BigNat,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<Vec<usize>, Error> {
        let reachable = self.reachability(requested_words);
        if !reachable[requested_words][self.start] {
            return Err(Error::NoRealization(
                "corpus transition model cannot terminate at that word count",
            ));
        }
        let mut current = self.start;
        let mut sequence = Vec::with_capacity(requested_words);
        for remaining in (1..=requested_words).rev() {
            let candidates: Vec<_> = self.transitions[current]
                .iter()
                .copied()
                .filter(|transition| {
                    transition.to != self.end && reachable[remaining - 1][transition.to]
                })
                .collect();
            current = select_transition(residual, &candidates, trace)?;
            sequence.push(current);
        }
        Ok(sequence)
    }

    fn reachability(&self, requested_words: usize) -> Vec<Vec<bool>> {
        let states = self.transitions.len();
        let mut reachable = vec![vec![false; states]; requested_words + 1];
        for (state, candidates) in self.transitions.iter().enumerate() {
            reachable[0][state] = candidates.iter().any(|candidate| candidate.to == self.end);
        }
        for remaining in 1..=requested_words {
            for state in 0..states {
                reachable[remaining][state] = self.transitions[state].iter().any(|candidate| {
                    candidate.to != self.end && reachable[remaining - 1][candidate.to]
                });
            }
        }
        reachable
    }
}

fn select_transition(
    residual: &mut BigNat,
    candidates: &[Transition],
    trace: &mut Vec<TraceEvent>,
) -> Result<usize, Error> {
    let total: u64 = candidates
        .iter()
        .map(|candidate| candidate.weight as u64)
        .sum();
    if total == 0 || total > u32::MAX as u64 {
        return Err(Error::Data("invalid transition weight total".into()));
    }
    let before = residual.to_string();
    let roll = residual.div_rem_small(total as u32);
    let mut boundary = 0u32;
    let selected = candidates
        .iter()
        .find(|candidate| {
            boundary = boundary.saturating_add(candidate.weight);
            roll < boundary
        })
        .ok_or_else(|| Error::Data("weighted transition interval was incomplete".into()))?;
    trace.push(TraceEvent {
        field: "english_tag",
        radix: total as u32,
        index: roll,
        candidate_id: selected.to as u32,
        residual_before: before,
        residual_after: residual.to_string(),
    });
    Ok(selected.to)
}

fn select_word<'a>(
    residual: &mut BigNat,
    candidates: &'a [LexicalForm],
    trace: &mut Vec<TraceEvent>,
) -> Result<&'a LexicalForm, Error> {
    let total: u64 = candidates
        .iter()
        .map(|candidate| candidate.weight as u64)
        .sum();
    if total == 0 || total > u32::MAX as u64 {
        return Err(Error::Data("invalid lexical weight total".into()));
    }
    let before = residual.to_string();
    let roll = residual.div_rem_small(total as u32);
    let mut boundary = 0u32;
    let (candidate_id, selected) = candidates
        .iter()
        .enumerate()
        .find(|(_, candidate)| {
            boundary = boundary.saturating_add(candidate.weight);
            roll < boundary
        })
        .ok_or_else(|| Error::Data("weighted lexical interval was incomplete".into()))?;
    trace.push(TraceEvent {
        field: "english_word",
        radix: total as u32,
        index: roll,
        candidate_id: candidate_id as u32,
        residual_before: before,
        residual_after: residual.to_string(),
    });
    Ok(selected)
}

fn extend_dictionary(forms: &mut BTreeMap<String, u32>, dictionary: &[String]) {
    for word in dictionary {
        forms.entry(word.clone()).or_insert(1);
    }
}

fn dictionary_open_tag(tag: &str) -> bool {
    matches!(tag, "NN" | "VB" | "JJ" | "RB")
}

fn load_wordnet_index(path: &Path) -> Result<Vec<String>, Error> {
    let contents = fs::read_to_string(path).map_err(|error| data_error(path, error))?;
    let mut words = BTreeSet::new();
    for line in contents.lines() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(lemma) = line.split_whitespace().next() else {
            continue;
        };
        // WordNet writes lexicalized phrases with underscores. Hyphenating
        // them preserves each entry while keeping CELM's orthographic word
        // count stable.
        let lemma = lemma.to_ascii_lowercase().replace('_', "-");
        if is_single_dictionary_word(&lemma) {
            words.insert(lemma);
        }
    }
    if words.is_empty() {
        return Err(Error::Data(format!(
            "{} contained no lemmas",
            path.display()
        )));
    }
    Ok(words.into_iter().collect())
}

fn is_single_dictionary_word(word: &str) -> bool {
    word.chars()
        .any(|character| character.is_ascii_alphabetic())
        && word.chars().all(|character| {
            character.is_ascii_alphabetic() || character == '\'' || character == '-'
        })
}

fn is_brown_text_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.len() == 4
                && name.starts_with('c')
                && name[1..]
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
}

fn normalize_corpus_word(word: &str) -> String {
    let normalized = word.to_ascii_lowercase();
    if normalized
        .chars()
        .any(|character| character.is_ascii_alphabetic())
        && normalized.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '\'' | '-' | '.' | '&')
        })
    {
        normalized
    } else {
        String::new()
    }
}

fn normalize_tag(tag: &str) -> String {
    tag.split('-').next().unwrap_or(tag).to_ascii_uppercase()
}

fn is_punctuation_tag(tag: &str) -> bool {
    !tag.chars().any(|character| character.is_ascii_alphabetic())
}

fn increment(counts: &mut BTreeMap<(String, String), u32>, from: &str, to: &str) {
    let count = counts.entry((from.into(), to.into())).or_default();
    *count = count.saturating_add(1);
}

fn render(meaning: &EnglishMeaning) -> String {
    let mut words: Vec<String> = meaning
        .words
        .iter()
        .map(|tagged| tagged.word.clone())
        .collect();
    for index in 0..words.len().saturating_sub(1) {
        let begins_with_vowel = words[index + 1].chars().next().is_some_and(|letter| {
            matches!(letter.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u')
        });
        if words[index] == "a" && begins_with_vowel {
            words[index] = "an".into();
        } else if words[index] == "an" && !begins_with_vowel {
            words[index] = "a".into();
        }
    }
    let mut sentence = words.join(" ");
    if let Some(first) = sentence.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    sentence.push('.');
    sentence
}

fn data_error(path: &Path, error: std::io::Error) -> Error {
    Error::Data(format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn profile() -> &'static EnglishProfile {
        static PROFILE: OnceLock<EnglishProfile> = OnceLock::new();
        PROFILE.get_or_init(|| EnglishProfile::load_default().unwrap())
    }

    #[test]
    fn loads_full_data_profile() {
        let stats = profile().stats();
        assert!(stats.wordnet_nouns > 50_000, "{stats:?}");
        assert!(stats.wordnet_verbs > 8_000, "{stats:?}");
        assert!(stats.corpus_sentences > 50_000, "{stats:?}");
        assert!(stats.corpus_tokens > 900_000, "{stats:?}");
        assert!(stats.lexical_forms > 100_000, "{stats:?}");
    }

    #[test]
    fn exact_lengths_are_deterministic_verified_and_reversible() {
        let choice = ChoiceState::from_hex(
            256,
            "8F2A00000000000000000000000000000000000000000000000000000000D91C",
        )
        .unwrap();
        for words in [3, 4, 8, 12, 20, 40] {
            let first = profile().generate(&choice, words).unwrap();
            let second = profile().generate(&choice, words).unwrap();
            assert_eq!(first, second);
            assert_eq!(first.sentence.split_whitespace().count(), words);
            assert!(profile().verify(&first));
            assert_eq!(profile().recover_integer(&first).unwrap(), choice.integer);
        }
    }
}
