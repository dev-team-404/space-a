//! R6 채굴 강화(A) — 정규화 프롬프트(norm60)의 느슨한 군집화(재현율 위주).
//! 문자 bigram Jaccard 유사도로 "살짝 다르게 쓴 같은 지시"를 한 묶음으로 모은다.
//! 정밀도는 상위 R6 LLM 판정이 담당하므로 threshold는 낮게(느슨하게) 잡는다.
//! LLM·네트워크 무관 순수 로직 — 한국어 조사 변형(코멘트/코멘트를)에도 문자 단위라 강건.

use std::collections::HashSet;

/// 문자 bigram 집합. 공백 붕괴된 norm60 기준. 길이 1이면 그 문자를 유니그램으로(폴백).
fn bigrams(s: &str) -> HashSet<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = HashSet::new();
    if chars.len() < 2 {
        if let Some(&c) = chars.first() {
            set.insert((c, c));
        }
        return set;
    }
    for w in chars.windows(2) {
        set.insert((w[0], w[1]));
    }
    set
}

/// 두 bigram 집합의 Jaccard 유사도 = |∩| / |∪|. 한쪽이라도 비면 0.
fn jaccard(a: &HashSet<(char, char)>, b: &HashSet<(char, char)>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// union-find 루트(경로 압축).
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// norm60 목록을 느슨하게 군집화. sim ≥ threshold 인 쌍을 같은 묶음으로(union-find).
/// 반환: 각 묶음 = 인덱스 오름차순 Vec, 묶음들은 최소 인덱스 오름차순. 결정론(입력 순서 의존).
pub(crate) fn cluster_norms(norms: &[String], threshold: f64) -> Vec<Vec<usize>> {
    let n = norms.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let grams: Vec<HashSet<(char, char)>> = norms.iter().map(|s| bigrams(s)).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if jaccard(&grams[i], &grams[j]) >= threshold {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    // 작은 루트로 병합 → 앵커(최소 인덱스) 안정성.
                    let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                    parent[hi] = lo;
                }
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clusters_paraphrases_sharing_most_chars() {
        // "pr 리뷰 코멘트 종합 검토..." 변형 3개 — 대부분 문자를 공유 → 한 묶음.
        let norms = v(&[
            "pr 리뷰 코멘트 종합 검토해서 조치해줘",
            "pr 리뷰 코멘트 종합 검토하고 반영해줘",
            "pr 리뷰 코멘트 종합 검토 후 조치",
        ]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters.len(), 1, "패러프레이즈 3개는 한 묶음");
        assert_eq!(clusters[0], vec![0, 1, 2]);
    }

    #[test]
    fn separates_unrelated_norms() {
        let norms = v(&[
            "매일 아침 판매 리포트 뽑아줘",
            "도커 컨테이너 로그 확인해줘",
            "리액트 상태관리 코드 리팩터",
        ]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters.len(), 3, "관련 없는 지시는 각자 싱글턴");
    }

    #[test]
    fn identical_norms_cluster_together() {
        let norms = v(&["같은 지시를 두 번 넣었다", "같은 지시를 두 번 넣었다"]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters, vec![vec![0, 1]]);
    }

    #[test]
    fn deterministic_min_index_anchor_ordering() {
        // 0과 2가 유사, 1은 별개 → 묶음 [0,2] 는 인덱스 오름차순, 그리고 [1] 보다 먼저(최소 인덱스 0).
        let norms = v(&[
            "리뷰 코멘트 종합 검토해줘",
            "완전히 다른 무관한 작업 지시",
            "리뷰 코멘트 종합 검토 부탁",
        ]);
        let clusters = cluster_norms(&norms, 0.5);
        assert_eq!(clusters, vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(cluster_norms(&[], 0.5).is_empty());
    }
}
