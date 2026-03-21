use lru::LruCache;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum TextCompleteness {
    Complete,
    MissingPrefix,
    MissingSuffix,
    MissingBoth,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct VersionComparison {
    pub similarity_score: f64,
    pub new_completeness: TextCompleteness,
    pub should_replace: bool,
    pub reason: String,
}

const LCS_MAX_CHARS_EACH: usize = 1400;
const LCS_MAX_PRODUCT: usize = 1_600_000;
const FIND_BEST_CANDIDATE_BUDGET_MS: u64 = 18;
const FIND_BEST_CANDIDATE_BUDGET_MIN_MS: u64 = 12;
const FIND_BEST_CANDIDATE_BUDGET_MAX_MS: u64 = 30;
const CANDIDATE_LEN_RATIO_MIN: f64 = 0.22;
const CANDIDATE_EDGE_MATCH_MIN: f64 = 0.06;
// 优化：n-gram 相似度计算常量
const NGRAM_SIZE: usize = 3;
const NGRAM_SIMILARITY_THRESHOLD: f64 = 0.3;
static FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS: AtomicU64 =
    AtomicU64::new(FIND_BEST_CANDIDATE_BUDGET_MS);
static DEDUP_SCAN_TOTAL: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_ELAPSED_TOTAL_MS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_ITEMS_TOTAL: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_LAST_ELAPSED_MS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_LAST_SCANNED_ITEMS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_LAST_TIMEOUT: AtomicU64 = AtomicU64::new(0);
const VERSION_COMPARE_CACHE_CAPACITY: usize = 4096;

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
struct VersionCompareCacheKey {
    old_hash: u64,
    new_hash: u64,
    old_len: usize,
    new_len: usize,
    threshold_bits: u64,
}

static VERSION_COMPARE_CACHE: LazyLock<Mutex<LruCache<VersionCompareCacheKey, VersionComparison>>> =
    LazyLock::new(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(VERSION_COMPARE_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
        ))
    });

#[derive(Serialize)]
pub struct DedupScanMetrics {
    pub budget_ms_current: u64,
    pub total_scans: u64,
    pub timeout_scans: u64,
    pub timeout_ratio: f64,
    pub avg_elapsed_ms: f64,
    pub avg_scanned_items: f64,
    pub last_elapsed_ms: u64,
    pub last_scanned_items: u64,
    pub last_timeout: bool,
}

pub fn get_dedup_scan_metrics() -> DedupScanMetrics {
    let total_scans = DEDUP_SCAN_TOTAL.load(Ordering::Relaxed);
    let timeout_scans = DEDUP_SCAN_TIMEOUTS.load(Ordering::Relaxed);
    let elapsed_total = DEDUP_SCAN_ELAPSED_TOTAL_MS.load(Ordering::Relaxed);
    let items_total = DEDUP_SCAN_ITEMS_TOTAL.load(Ordering::Relaxed);
    let timeout_ratio = if total_scans == 0 {
        0.0
    } else {
        timeout_scans as f64 / total_scans as f64
    };
    let avg_elapsed_ms = if total_scans == 0 {
        0.0
    } else {
        elapsed_total as f64 / total_scans as f64
    };
    let avg_scanned_items = if total_scans == 0 {
        0.0
    } else {
        items_total as f64 / total_scans as f64
    };
    DedupScanMetrics {
        budget_ms_current: FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS.load(Ordering::Relaxed),
        total_scans,
        timeout_scans,
        timeout_ratio,
        avg_elapsed_ms,
        avg_scanned_items,
        last_elapsed_ms: DEDUP_SCAN_LAST_ELAPSED_MS.load(Ordering::Relaxed),
        last_scanned_items: DEDUP_SCAN_LAST_SCANNED_ITEMS.load(Ordering::Relaxed),
        last_timeout: DEDUP_SCAN_LAST_TIMEOUT.load(Ordering::Relaxed) == 1,
    }
}

fn prefix_match_ratio(text1: &str, text2: &str, sample_len: usize) -> f64 {
    let a: Vec<char> = text1.chars().take(sample_len).collect();
    let b: Vec<char> = text2.chars().take(sample_len).collect();
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut same = 0usize;
    for i in 0..n {
        if a[i] == b[i] {
            same += 1;
        }
    }
    same as f64 / n as f64
}

fn suffix_match_ratio(text1: &str, text2: &str, sample_len: usize) -> f64 {
    let mut a: Vec<char> = text1.chars().rev().take(sample_len).collect();
    let mut b: Vec<char> = text2.chars().rev().take(sample_len).collect();
    a.reverse();
    b.reverse();
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut same = 0usize;
    for i in 0..n {
        if a[i] == b[i] {
            same += 1;
        }
    }
    same as f64 / n as f64
}

/// 优化：计算 n-gram 相似度（比 LCS 更快）
fn ngram_similarity(text1: &str, text2: &str, n: usize) -> f64 {
    if text1.is_empty() && text2.is_empty() {
        return 1.0;
    }
    if text1.is_empty() || text2.is_empty() {
        return 0.0;
    }

    let chars1: Vec<char> = text1.chars().collect();
    let chars2: Vec<char> = text2.chars().collect();

    if chars1.len() < n || chars2.len() < n {
        // 文本太短，使用简单的字符匹配
        let min_len = chars1.len().min(chars2.len());
        if min_len == 0 {
            return 0.0;
        }
        let mut matches = 0;
        for i in 0..min_len {
            if chars1[i] == chars2[i] {
                matches += 1;
            }
        }
        return matches as f64 / min_len as f64;
    }

    // 构建 n-gram 集合
    let mut ngrams1 = std::collections::HashSet::new();
    let mut ngrams2 = std::collections::HashSet::new();

    for i in 0..=chars1.len() - n {
        let ngram: String = chars1[i..i + n].iter().collect();
        ngrams1.insert(ngram);
    }

    for i in 0..=chars2.len() - n {
        let ngram: String = chars2[i..i + n].iter().collect();
        ngrams2.insert(ngram);
    }

    // 计算 Jaccard 相似度
    let intersection = ngrams1.intersection(&ngrams2).count();
    let union = ngrams1.union(&ngrams2).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn calculate_text_similarity_fast(text1: &str, text2: &str, len1: usize, len2: usize) -> f64 {
    if text1 == text2 {
        return 1.0;
    }
    let max_len = len1.max(len2) as f64;
    let min_len = len1.min(len2) as f64;
    let length_ratio = if max_len == 0.0 { 0.0 } else { min_len / max_len };
    if text1.contains(text2) || text2.contains(text1) {
        return length_ratio.max(0.85);
    }
    let head = prefix_match_ratio(text1, text2, 256);
    let tail = suffix_match_ratio(text1, text2, 256);
    (head * 0.35 + tail * 0.35 + length_ratio * 0.30).min(1.0)
}

pub fn calculate_text_similarity(text1: &str, text2: &str) -> f64 {
    if text1.is_empty() && text2.is_empty() {
        return 1.0;
    }
    if text1.is_empty() || text2.is_empty() {
        return 0.0;
    }
    let chars1: Vec<char> = text1.chars().collect();
    let chars2: Vec<char> = text2.chars().collect();
    let len1 = chars1.len();
    let len2 = chars2.len();
    if len1 > LCS_MAX_CHARS_EACH
        || len2 > LCS_MAX_CHARS_EACH
        || len1.saturating_mul(len2) > LCS_MAX_PRODUCT
    {
        return calculate_text_similarity_fast(text1, text2, len1, len2);
    }
    let mut dp = vec![vec![0; len2 + 1]; len1 + 1];
    for i in 1..=len1 {
        for j in 1..=len2 {
            if chars1[i - 1] == chars2[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    let lcs_length = dp[len1][len2];
    let max_len = len1.max(len2);
    if max_len == 0 {
        0.0
    } else {
        lcs_length as f64 / max_len as f64
    }
}

fn candidate_prefilter(old_text: &str, new_text: &str) -> bool {
    if old_text.is_empty() || new_text.is_empty() {
        return true;
    }
    if old_text.contains(new_text) || new_text.contains(old_text) {
        return true;
    }
    let len_old = old_text.chars().count();
    let len_new = new_text.chars().count();
    let min_len = len_old.min(len_new) as f64;
    let max_len = len_old.max(len_new) as f64;
    if max_len > 0.0 && (min_len / max_len) < CANDIDATE_LEN_RATIO_MIN {
        return false;
    }

    // 优化：使用 n-gram 相似度进行快速预筛选（比 LCS 快得多）
    let ngram_sim = ngram_similarity(old_text, new_text, NGRAM_SIZE);
    if ngram_sim >= NGRAM_SIMILARITY_THRESHOLD {
        return true;
    }

    // 回退到原有的前缀/后缀匹配
    let head = prefix_match_ratio(old_text, new_text, 32);
    let tail = suffix_match_ratio(old_text, new_text, 32);
    head >= CANDIDATE_EDGE_MATCH_MIN || tail >= CANDIDATE_EDGE_MATCH_MIN
}

pub fn detect_text_completeness(text: &str, reference_text: &str) -> TextCompleteness {
    let similarity = calculate_text_similarity(text, reference_text);
    detect_text_completeness_with_similarity(text, reference_text, similarity)
}

fn detect_text_completeness_with_similarity(
    text: &str,
    reference_text: &str,
    similarity: f64,
) -> TextCompleteness {
    if text.is_empty() || reference_text.is_empty() {
        return TextCompleteness::Unknown;
    }
    if text == reference_text {
        return TextCompleteness::Complete;
    }
    if text.len() > reference_text.len() {
        return TextCompleteness::Complete;
    }
    if reference_text.starts_with(text) {
        return TextCompleteness::MissingSuffix;
    }
    if reference_text.ends_with(text) {
        return TextCompleteness::MissingPrefix;
    }
    if reference_text.contains(text) && text.len() < reference_text.len() {
        return TextCompleteness::MissingBoth;
    }
    if similarity > 0.8 {
        let text_chars: Vec<char> = text.chars().collect();
        let ref_chars: Vec<char> = reference_text.chars().collect();
        let mut prefix_match = true;
        let min_len = text_chars.len().min(10);
        for i in 0..min_len {
            if i >= ref_chars.len() || text_chars[i] != ref_chars[i] {
                prefix_match = false;
                break;
            }
        }
        let mut suffix_match = true;
        let min_len = text_chars.len().min(10);
        for i in 0..min_len {
            let text_idx = text_chars.len() - 1 - i;
            let ref_idx = ref_chars.len() - 1 - i;
            if text_idx >= text_chars.len()
                || ref_idx >= ref_chars.len()
                || text_chars[text_idx] != ref_chars[ref_idx]
            {
                suffix_match = false;
                break;
            }
        }
        match (prefix_match, suffix_match) {
            (true, false) => TextCompleteness::MissingSuffix,
            (false, true) => TextCompleteness::MissingPrefix,
            (false, false) => TextCompleteness::MissingBoth,
            (true, true) => TextCompleteness::Complete,
        }
    } else {
        TextCompleteness::Unknown
    }
}

fn count_punctuation(text: &str) -> usize {
    let punctuation_chars = ['。', '！', '？', '.', '!', '?', '；', ';', '，', ','];
    text.chars().filter(|&c| punctuation_chars.contains(&c)).count()
}

fn is_more_complete_sentence(new_text: &str, old_text: &str) -> bool {
    let new_ends_with_period = has_sentence_endings(new_text);
    let old_ends_with_period = has_sentence_endings(old_text);
    new_ends_with_period && !old_ends_with_period
}

fn has_sentence_endings(text: &str) -> bool {
    let ending_chars = ['。', '！', '？', '.', '!', '?'];
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|c| ending_chars.contains(&c))
}

fn is_truncated_sentence(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return false;
    }
    let last_char = trimmed.chars().last().unwrap_or_default();
    let truncation_indicators = ['，', ',', '、', '(', '[', '{', '"', '\''];
    truncation_indicators.contains(&last_char)
        || (!has_sentence_endings(trimmed)
        && (trimmed.ends_with("但非")
        || trimmed.ends_with("但是")
        || trimmed.ends_with("而且")
        || trimmed.ends_with("并且")))
}

fn is_subset_of(new_text: &str, old_text: &str) -> bool {
    if new_text.is_empty() || old_text.is_empty() {
        return false;
    }
    if old_text.starts_with(new_text) {
        return true;
    }
    if old_text.ends_with(new_text) {
        return true;
    }
    old_text.contains(new_text) && new_text.len() < old_text.len()
}

fn stable_text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn compare_versions(old_text: &str, new_text: &str, similarity_threshold: f64) -> VersionComparison {
    let cache_key = VersionCompareCacheKey {
        old_hash: stable_text_hash(old_text),
        new_hash: stable_text_hash(new_text),
        old_len: old_text.len(),
        new_len: new_text.len(),
        threshold_bits: similarity_threshold.to_bits(),
    };
    if let Some(hit) = VERSION_COMPARE_CACHE.lock().get(&cache_key).cloned() {
        return hit;
    }
    let similarity = calculate_text_similarity(old_text, new_text);
    let completeness = detect_text_completeness_with_similarity(new_text, old_text, similarity);
    let (should_replace, reason) = if similarity >= similarity_threshold {
        match completeness {
            TextCompleteness::Complete => {
                if new_text.len() > old_text.len() {
                    (true, "新版本更完整，长度更长".to_string())
                } else if new_text.len() == old_text.len() {
                    let new_has_more_punctuation =
                        count_punctuation(new_text) > count_punctuation(old_text);
                    let new_is_more_complete = is_more_complete_sentence(new_text, old_text);
                    if new_has_more_punctuation || new_is_more_complete {
                        (true, "新版本句子结构更完整".to_string())
                    } else {
                        (false, "版本相同，无需替换".to_string())
                    }
                } else if is_subset_of(new_text, old_text) {
                    (true, "新版本是已有完整版本的子集，移动完整版本到前面".to_string())
                } else {
                    let old_is_truncated = is_truncated_sentence(old_text);
                    let new_is_complete = has_sentence_endings(new_text);
                    if old_is_truncated && new_is_complete {
                        (true, "替换不完整的截断版本".to_string())
                    } else {
                        (false, "新版本较短，保持原版本".to_string())
                    }
                }
            }
            TextCompleteness::MissingPrefix
            | TextCompleteness::MissingSuffix
            | TextCompleteness::MissingBoth => {
                if new_text.len() < old_text.len() && is_subset_of(new_text, old_text) {
                    (true, "找回完整版本，将完整版本移动到前面".to_string())
                } else if new_text.len() > old_text.len() && has_sentence_endings(new_text) {
                    (true, "新版本虽被标记为不完整但实际更完整".to_string())
                } else {
                    (false, "新版本内容不完整，保持原版本".to_string())
                }
            }
            TextCompleteness::Unknown => {
                if new_text.len() > old_text.len()
                    && has_sentence_endings(new_text)
                    && !has_sentence_endings(old_text)
                {
                    (true, "基于长度和句子完整性判断，新版本更完整".to_string())
                } else {
                    (false, "无法确定版本关系，保持原版本".to_string())
                }
            }
        }
    } else {
        (false, "文本相似度低于阈值，视为不同内容".to_string())
    };

    let result = VersionComparison {
        similarity_score: similarity,
        new_completeness: completeness,
        should_replace,
        reason,
    };
    VERSION_COMPARE_CACHE
        .lock()
        .put(cache_key, result.clone());
    result
}

pub fn find_best_replacement_candidate(
    new_text: &str,
    history: &[String],
    similarity_threshold: f64,
) -> Option<(usize, VersionComparison)> {
    let mut best_candidate: Option<(usize, VersionComparison)> = None;
    let started = Instant::now();
    let budget_ms = FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS
        .load(Ordering::Relaxed)
        .clamp(
            FIND_BEST_CANDIDATE_BUDGET_MIN_MS,
            FIND_BEST_CANDIDATE_BUDGET_MAX_MS,
        );
    let budget = Duration::from_millis(budget_ms);
    let mut scanned = 0usize;
    let mut timed_out = false;

    for (index, old_text) in history.iter().enumerate() {
        if started.elapsed() >= budget {
            timed_out = true;
            break;
        }
        if !candidate_prefilter(old_text, new_text) {
            continue;
        }
        scanned += 1;
        let comparison = compare_versions(old_text, new_text, similarity_threshold);
        if comparison.should_replace {
            match &best_candidate {
                None => best_candidate = Some((index, comparison)),
                Some((_, existing_comparison)) => {
                    if comparison.similarity_score > existing_comparison.similarity_score
                        || (comparison.similarity_score == existing_comparison.similarity_score
                        && (matches!(comparison.new_completeness, TextCompleteness::Complete)
                        || comparison.reason.contains("更完整")))
                    {
                        best_candidate = Some((index, comparison));
                    }
                }
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    DEDUP_SCAN_TOTAL.fetch_add(1, Ordering::Relaxed);
    DEDUP_SCAN_ELAPSED_TOTAL_MS.fetch_add(elapsed_ms, Ordering::Relaxed);
    DEDUP_SCAN_ITEMS_TOTAL.fetch_add(scanned as u64, Ordering::Relaxed);
    DEDUP_SCAN_LAST_ELAPSED_MS.store(elapsed_ms, Ordering::Relaxed);
    DEDUP_SCAN_LAST_SCANNED_ITEMS.store(scanned as u64, Ordering::Relaxed);
    DEDUP_SCAN_LAST_TIMEOUT.store(if timed_out { 1 } else { 0 }, Ordering::Relaxed);
    if timed_out {
        DEDUP_SCAN_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
    }

    let next_budget_ms = if timed_out {
        (budget_ms + 2).min(FIND_BEST_CANDIDATE_BUDGET_MAX_MS)
    } else if elapsed_ms.saturating_mul(2) < budget_ms {
        budget_ms
            .saturating_sub(1)
            .max(FIND_BEST_CANDIDATE_BUDGET_MIN_MS)
    } else {
        budget_ms
    };
    if next_budget_ms != budget_ms {
        FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS.store(next_budget_ms, Ordering::Relaxed);
    }

    best_candidate
}
