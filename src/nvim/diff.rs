#[derive(Debug, PartialEq)]
pub enum DiffChange {
    Equal(String),
    Add(String),
    Delete(String),
}

/// 2つのテキスト（行リスト）を比較して、差分のリストを返す
pub fn compute_diff(old: &[String], new: &[String]) -> Vec<DiffChange> {
    let m = old.len();
    let n = new.len();
    
    // DP table for LCS
    let mut dp = vec![vec![0; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old[i-1] == new[j-1] {
                dp[i][j] = dp[i-1][j-1] + 1;
            } else {
                dp[i][j] = dp[i-1][j].max(dp[i][j-1]);
            }
        }
    }

    // Backtrack to build DiffChange list
    let mut res = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i-1] == new[j-1] {
            res.push(DiffChange::Equal(old[i-1].clone()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j-1] >= dp[i-1][j]) {
            res.push(DiffChange::Add(new[j-1].clone()));
            j -= 1;
        } else {
            res.push(DiffChange::Delete(old[i-1].clone()));
            i -= 1;
        }
    }
    res.reverse();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_engine_exhaustive() {
        // Equal
        let res = compute_diff(&["a".into()], &["a".into()]);
        assert_eq!(res, vec![DiffChange::Equal("a".into())]);

        // Add
        let res = compute_diff(&[], &["a".into()]);
        assert_eq!(res, vec![DiffChange::Add("a".into())]);

        // Delete
        let res = compute_diff(&["a".into()], &[]);
        assert_eq!(res, vec![DiffChange::Delete("a".into())]);

        // Typical mixed
        let old = vec!["A".into(), "B".into(), "C".into()];
        let new = vec!["A".into(), "C".into(), "D".into()];
        let res = compute_diff(&old, &new);
        assert_eq!(res.len(), 4);
        assert_eq!(res[0], DiffChange::Equal("A".into()));
        assert_eq!(res[1], DiffChange::Delete("B".into()));
        assert_eq!(res[2], DiffChange::Equal("C".into()));
        assert_eq!(res[3], DiffChange::Add("D".into()));
    }
}
