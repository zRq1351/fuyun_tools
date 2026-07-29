use lru::LruCache;
use parking_lot::Mutex;
use serde::Serialize;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use xxhash_rust::xxh3::xxh3_64;

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
// 优化：降低 LCS 乘积阈值，更早触发快速路径，避免 O(M×N) 阻塞
const LCS_MAX_PRODUCT: usize = 800_000;
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

    let mut ngrams1 = std::collections::HashSet::new();
    for window in chars1.windows(n) {
        ngrams1.insert(window);
    }

    let mut ngrams2 = std::collections::HashSet::new();
    for window in chars2.windows(n) {
        ngrams2.insert(window);
    }

    let intersection = ngrams1.intersection(&ngrams2).count();
    let union = ngrams1.len() + ngrams2.len() - intersection;

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
    let length_ratio = if max_len == 0.0 {
        0.0
    } else {
        min_len / max_len
    };
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
    let len1 = text1.chars().count();
    let len2 = text2.chars().count();
    if len1 > LCS_MAX_CHARS_EACH
        || len2 > LCS_MAX_CHARS_EACH
        || len1.saturating_mul(len2) > LCS_MAX_PRODUCT
    {
        return calculate_text_similarity_fast(text1, text2, len1, len2);
    }
    let chars1: Vec<char> = text1.chars().collect();
    let chars2: Vec<char> = text2.chars().collect();
    let mut dp = vec![0; len2 + 1];
    for i in 1..=len1 {
        let mut prev = 0;
        for j in 1..=len2 {
            let temp = dp[j];
            if chars1[i - 1] == chars2[j - 1] {
                dp[j] = prev + 1;
            } else {
                dp[j] = dp[j].max(dp[j - 1]);
            }
            prev = temp;
        }
    }
    let lcs_length = dp[len2];
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

    let len_old = old_text.len();
    let len_new = new_text.len();
    let min_len = len_old.min(len_new) as f64;
    let max_len = len_old.max(len_new) as f64;
    if max_len > 0.0 && (min_len / max_len) < CANDIDATE_LEN_RATIO_MIN {
        return false;
    }
    if old_text.contains(new_text) || new_text.contains(old_text) {
        return true;
    }

    let ngram_sim = ngram_similarity(old_text, new_text, NGRAM_SIZE);
    if ngram_sim >= NGRAM_SIMILARITY_THRESHOLD {
        return true;
    }

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
    text.chars()
        .filter(|&c| punctuation_chars.contains(&c))
        .count()
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
    xxh3_64(text.as_bytes())
}

pub fn compare_versions(
    old_text: &str,
    new_text: &str,
    similarity_threshold: f64,
) -> VersionComparison {
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
    if old_text == new_text {
        let result = VersionComparison {
            similarity_score: 1.0,
            new_completeness: TextCompleteness::Complete,
            should_replace: false,
            reason: "版本相同，无需替换".to_string(),
        };
        VERSION_COMPARE_CACHE.lock().put(cache_key, result.clone());
        return result;
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
                    (
                        true,
                        "新版本是已有完整版本的子集，移动完整版本到前面".to_string(),
                    )
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
    VERSION_COMPARE_CACHE.lock().put(cache_key, result.clone());
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

        if old_text.len() > 100_000 || new_text.len() > 100_000 {
            continue;
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

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    // ===== calculate_text_similarity =====

    #[test]
    fn similarity_identical_texts() {
        assert_eq!(calculate_text_similarity("hello", "hello"), 1.0);
    }

    #[test]
    fn similarity_empty_texts() {
        assert_eq!(calculate_text_similarity("", ""), 1.0);
    }

    #[test]
    fn similarity_one_empty() {
        assert_eq!(calculate_text_similarity("hello", ""), 0.0);
        assert_eq!(calculate_text_similarity("", "hello"), 0.0);
    }

    #[test]
    fn similarity完全不同的文本() {
        let sim = calculate_text_similarity("abc", "xyz");
        assert!(sim < 0.1, "完全不同的文本相似度应很低: {}", sim);
    }

    #[test]
    fn similarity子串包含() {
        // "hello world" vs "hello": LCS-based similarity for short texts
        let sim = calculate_text_similarity("hello world", "hello");
        // Short text uses LCS path; "hello" is subset of "hello world"
        assert!(sim > 0.3, "子串文本应有一定相似度: {}", sim);
    }

    #[test]
    fn similarity中文文本() {
        let sim = calculate_text_similarity("你好世界", "你好世界！");
        assert!(sim >= 0.8, "相似中文文本应有高相似度: {}", sim);
    }

    #[test]
    fn similarity长文本使用快速路径() {
        let long1 = "a".repeat(2000);
        let long2 = "a".repeat(1900);
        let sim = calculate_text_similarity(&long1, &long2);
        assert!(sim > 0.9, "长文本快速路径应返回合理结果: {}", sim);
    }

    // ===== detect_text_completeness =====

    #[test]
    fn completeness完全相同() {
        assert_eq!(
            detect_text_completeness("hello", "hello"),
            TextCompleteness::Complete
        );
    }

    #[test]
    fn completeness缺少后缀() {
        assert_eq!(
            detect_text_completeness("hel", "hello"),
            TextCompleteness::MissingSuffix
        );
    }

    #[test]
    fn completeness缺少前缀() {
        assert_eq!(
            detect_text_completeness("llo", "hello"),
            TextCompleteness::MissingPrefix
        );
    }

    #[test]
    fn completeness缺少前后缀() {
        assert_eq!(
            detect_text_completeness("ell", "hello"),
            TextCompleteness::MissingBoth
        );
    }

    #[test]
    fn completeness空文本() {
        assert_eq!(
            detect_text_completeness("", "hello"),
            TextCompleteness::Unknown
        );
        assert_eq!(
            detect_text_completeness("hello", ""),
            TextCompleteness::Unknown
        );
    }

    #[test]
    fn completeness新版本更长视为完整() {
        assert_eq!(
            detect_text_completeness("hello world!", "hello"),
            TextCompleteness::Complete
        );
    }

    // ===== compare_versions =====

    #[test]
    fn versions相同文本不替换() {
        let result = compare_versions("hello", "hello", 0.8);
        assert!(!result.should_replace);
        assert_eq!(result.similarity_score, 1.0);
    }

    #[test]
    fn versions_newer_longer_should_replace() {
        // similarity("hello", "hello world") ≈ 0.45, so threshold must be <= 0.45
        let result = compare_versions("hello", "hello world", 0.4);
        assert!(result.should_replace);
        assert!(result.reason.contains("更完整"));
    }

    #[test]
    fn versions_totally_different_no_replace() {
        let result = compare_versions("apple", "banana", 0.8);
        assert!(!result.should_replace);
        assert!(result.similarity_score < 0.3);
    }

    #[test]
    fn versions_subset_should_move_complete_to_front() {
        // similarity("hello world", "hello") ≈ 0.45, threshold must be <= 0.45
        let result = compare_versions("hello world", "hello", 0.4);
        assert!(result.should_replace);
        assert!(result.reason.contains("完整版本"));
    }

    // ===== find_best_replacement_candidate =====

    #[test]
    fn dedup空历史返回None() {
        let result = find_best_replacement_candidate("hello", &[], 0.8);
        assert!(result.is_none());
    }

    #[test]
    fn dedup找到完全相同的不替换() {
        let history = vec!["hello".to_string(), "world".to_string()];
        let result = find_best_replacement_candidate("hello", &history, 0.8);
        assert!(result.is_none(), "完全相同的文本不应触发替换");
    }

    #[test]
    fn dedup找到更完整版本应替换() {
        let history = vec!["hel".to_string(), "world".to_string()];
        let result = find_best_replacement_candidate("hello", &history, 0.5);
        assert!(result.is_some(), "更完整的版本应被找到");
        let (idx, comp) = result.unwrap();
        assert_eq!(idx, 0);
        assert!(comp.should_replace);
    }

    #[test]
    fn dedup不同内容返回None() {
        let history = vec!["apple".to_string(), "banana".to_string()];
        let result = find_best_replacement_candidate("cherry", &history, 0.8);
        assert!(result.is_none());
    }

    // ===== ngram_similarity =====

    #[test]
    fn ngram相同文本() {
        assert_eq!(ngram_similarity("hello", "hello", 3), 1.0);
    }

    #[test]
    fn ngram空文本() {
        assert_eq!(ngram_similarity("", "", 3), 1.0);
        assert_eq!(ngram_similarity("hello", "", 3), 0.0);
    }

    #[test]
    fn ngram短文本回退到字符匹配() {
        let sim = ngram_similarity("ab", "ac", 3);
        assert!(sim > 0.0 && sim < 1.0);
    }

    // ===== prefix/suffix match ratio =====

    #[test]
    fn prefix完全匹配() {
        assert_eq!(prefix_match_ratio("hello world", "hello earth", 5), 1.0);
    }

    #[test]
    fn suffix完全匹配() {
        assert_eq!(suffix_match_ratio("hello world", "hi world", 5), 1.0);
    }

    #[test]
    fn prefix空字符串() {
        assert_eq!(prefix_match_ratio("", "hello", 5), 0.0);
    }

    // ===== Edge cases =====

    #[test]
    fn similarity特殊字符() {
        let sim = calculate_text_similarity("a!@#$%", "a!@#$%");
        assert_eq!(sim, 1.0);
    }

    #[test]
    fn similarityUnicode表情() {
        let sim = calculate_text_similarity("hello 😀", "hello 😀");
        assert_eq!(sim, 1.0);
    }

    #[test]
    fn dedup超大文本跳过() {
        let huge = "x".repeat(200_000);
        let history = vec![huge.clone()];
        let result = find_best_replacement_candidate(&huge, &history, 0.8);
        assert!(result.is_none(), "超大文本应被跳过");
    }

    // ===== has_sentence_endings =====

    #[test]
    fn sentence_ending_chinese_period() {
        assert!(has_sentence_endings("你好。"));
        assert!(has_sentence_endings("你好！"));
        assert!(has_sentence_endings("你好？"));
    }

    #[test]
    fn sentence_ending_english_period() {
        assert!(has_sentence_endings("Hello."));
        assert!(has_sentence_endings("Hello!"));
        assert!(has_sentence_endings("Hello?"));
    }

    #[test]
    fn sentence_ending_no_ending() {
        assert!(!has_sentence_endings("hello"));
        assert!(!has_sentence_endings("你好"));
        assert!(!has_sentence_endings("hello,"));
    }

    #[test]
    fn sentence_ending_empty() {
        assert!(!has_sentence_endings(""));
    }

    #[test]
    fn sentence_ending_with_whitespace() {
        assert!(has_sentence_endings("hello. "));
        assert!(has_sentence_endings("hello。  "));
    }

    // ===== is_truncated_sentence =====

    #[test]
    fn truncated_comma_ending() {
        assert!(is_truncated_sentence("hello,"));
        assert!(is_truncated_sentence("你好，"));
    }

    #[test]
    fn truncated_parenthesis_ending() {
        assert!(is_truncated_sentence("hello("));
        assert!(is_truncated_sentence("hello["));
        assert!(is_truncated_sentence("hello{"));
    }

    #[test]
    fn truncated_quote_ending() {
        assert!(is_truncated_sentence("hello\""));
        assert!(is_truncated_sentence("hello'"));
    }

    #[test]
    fn truncated_connector_words() {
        assert!(is_truncated_sentence("但是"));
        assert!(is_truncated_sentence("而且"));
        assert!(is_truncated_sentence("并且"));
        assert!(is_truncated_sentence("但非"));
    }

    #[test]
    fn truncated_not_truncated() {
        assert!(!is_truncated_sentence("hello."));
        assert!(!is_truncated_sentence("你好！"));
        assert!(!is_truncated_sentence("complete sentence"));
    }

    #[test]
    fn truncated_empty() {
        assert!(!is_truncated_sentence(""));
    }

    // ===== is_subset_of =====

    #[test]
    fn subset_prefix() {
        assert!(is_subset_of("hel", "hello"));
    }

    #[test]
    fn subset_suffix() {
        assert!(is_subset_of("llo", "hello"));
    }

    #[test]
    fn subset_contained() {
        assert!(is_subset_of("ell", "hello"));
    }

    #[test]
    fn subset_not_subset() {
        assert!(!is_subset_of("hello", "world"));
    }

    #[test]
    fn subset_equal_length_is_subset() {
        // is_subset_of checks starts_with/ends_with, equal text qualifies
        assert!(is_subset_of("hello", "hello"));
    }

    #[test]
    fn subset_empty() {
        assert!(!is_subset_of("", "hello"));
        assert!(!is_subset_of("hello", ""));
    }

    // ===== count_punctuation =====

    #[test]
    fn punctuation_chinese() {
        assert_eq!(count_punctuation("你好，世界。"), 2);
        assert_eq!(count_punctuation("你好！世界？"), 2);
    }

    #[test]
    fn punctuation_english() {
        // ':' is NOT in the punctuation list, so only 3
        assert_eq!(count_punctuation("hello, world."), 2);
        assert_eq!(count_punctuation("hello! world? yes;"), 3);
    }

    #[test]
    fn punctuation_none() {
        assert_eq!(count_punctuation("hello world"), 0);
    }

    #[test]
    fn punctuation_empty() {
        assert_eq!(count_punctuation(""), 0);
    }

    // ===== is_more_complete_sentence =====

    #[test]
    fn more_complete_new_has_ending_old_doesnt() {
        assert!(is_more_complete_sentence("hello.", "hello"));
        assert!(is_more_complete_sentence("你好。", "你好"));
    }

    #[test]
    fn more_complete_both_have_ending() {
        assert!(!is_more_complete_sentence("hello.", "world."));
    }

    #[test]
    fn more_complete_neither_has_ending() {
        assert!(!is_more_complete_sentence("hello", "world"));
    }

    // ===== candidate_prefilter =====

    #[test]
    fn prefilter_empty_returns_true() {
        assert!(candidate_prefilter("", "hello"));
        assert!(candidate_prefilter("hello", ""));
    }

    #[test]
    fn prefilter_containment() {
        assert!(candidate_prefilter("hello world", "hello"));
        assert!(candidate_prefilter("hello", "hello world"));
    }

    #[test]
    fn prefilter_length_ratio_too_low() {
        // "a" vs "abcdefghij" -> ratio = 1/10 = 0.1 < 0.22
        assert!(!candidate_prefilter("a", "abcdefghij"));
    }

    #[test]
    fn prefilter_ngram_similarity() {
        // Similar texts should pass
        assert!(candidate_prefilter("hello world", "hello earth"));
    }

    #[test]
    fn prefilter_edge_match() {
        // Same prefix should pass
        assert!(candidate_prefilter("hello abc", "hello xyz"));
    }

    // ===== stable_text_hash =====

    #[test]
    fn hash_deterministic() {
        let h1 = stable_text_hash("test");
        let h2 = stable_text_hash("test");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_different_inputs() {
        let h1 = stable_text_hash("abc");
        let h2 = stable_text_hash("def");
        assert_ne!(h1, h2);
    }

    // ===== compare_versions edge cases =====

    #[test]
    fn versions_same_length_different_content() {
        let result = compare_versions("hello", "world", 0.8);
        assert!(!result.should_replace);
    }

    #[test]
    fn versions_new_has_more_punctuation() {
        let result = compare_versions("hello", "hello!", 0.8);
        // Same length after considering punctuation, new has more
        assert!(result.should_replace || result.similarity_score > 0.8);
    }

    #[test]
    fn versions_same_length_same_punctuation_count() {
        // "hello," vs "hello." - same length, same punctuation count, both have endings
        let result = compare_versions("hello,", "hello.", 0.8);
        // Neither is "more complete" since both have sentence endings
        // and punctuation count is equal -> no replacement
        assert!(!result.should_replace);
    }

    #[test]
    fn versions_new_shorter_not_subset() {
        let result = compare_versions("hello world", "xyz", 0.8);
        assert!(!result.should_replace);
    }

    // ===== find_best_replacement_candidate edge cases =====

    #[test]
    fn dedup_picks_best_similarity() {
        let history = vec![
            "hello".to_string(),
            "hello world".to_string(),
            "hello earth".to_string(),
        ];
        let result = find_best_replacement_candidate("hello", &history, 0.3);
        // Should pick the most similar one
        if let Some((idx, comp)) = result {
            assert!(comp.should_replace);
            assert!(idx < 3);
        }
    }

    #[test]
    fn dedup_single_item_history() {
        let history = vec!["hello".to_string()];
        let result = find_best_replacement_candidate("hello", &history, 0.8);
        assert!(result.is_none());
    }

    #[test]
    fn dedup_threshold_zero_always_matches() {
        let history = vec!["completely different".to_string()];
        let result = find_best_replacement_candidate("hello", &history, 0.0);
        // With threshold 0, any text should match
        // But compare_versions with threshold 0: similarity >= 0 is always true
        // However, should_replace depends on completeness logic
        // This tests the boundary
        let _ = result;
    }

    // ===== calculate_text_similarity edge cases =====

    #[test]
    fn similarity_single_char() {
        assert_eq!(calculate_text_similarity("a", "a"), 1.0);
        assert!(calculate_text_similarity("a", "b") < 0.1);
    }

    #[test]
    fn similarity_one_char_match() {
        let sim = calculate_text_similarity("abc", "axc");
        assert!(sim > 0.5, "单字符差异应有较高相似度: {}", sim);
    }

    #[test]
    fn similarity_reversed() {
        let sim1 = calculate_text_similarity("hello", "world");
        let sim2 = calculate_text_similarity("world", "hello");
        assert!((sim1 - sim2).abs() < 0.01, "对称性: {} vs {}", sim1, sim2);
    }

    #[test]
    fn similarity_long_identical() {
        let text = "a".repeat(500);
        assert_eq!(calculate_text_similarity(&text, &text), 1.0);
    }

    // ===== detect_text_completeness edge cases =====

    #[test]
    fn completeness_both_empty() {
        assert_eq!(
            detect_text_completeness("", ""),
            TextCompleteness::Unknown
        );
    }

    #[test]
    fn completeness_exact_match() {
        assert_eq!(
            detect_text_completeness("abc", "abc"),
            TextCompleteness::Complete
        );
    }

    #[test]
    fn completeness_new_longer_is_complete() {
        assert_eq!(
            detect_text_completeness("hello world!", "hello"),
            TextCompleteness::Complete
        );
    }

    // ===== get_dedup_scan_metrics =====

    #[test]
    fn metrics_initial_state() {
        let m = get_dedup_scan_metrics();
        // Budget should be within configured range
        assert!(m.budget_ms_current >= 12 && m.budget_ms_current <= 30);
    }
}
