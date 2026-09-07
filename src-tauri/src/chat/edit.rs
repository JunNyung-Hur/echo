//! str_replace 편집 엔진 — Meetzy chat_agent/tools.py 의 apply_str_edits +
//! 가드 헬퍼(visible_text / strip_comments / whitespace-tolerant 매칭) 1:1 이식.
//!
//! edits=[{old, new, replace_all}]. 각 old 는 본문에 *정확히 1회* 매칭해야
//! (맥락 포함해 유일). 정확 매칭 실패 시 *공백 관대* 매칭으로 폴백(들여쓰기/줄바꿈
//! 차이 무시). 0회=못찾음, 2+회=모호 → 에러로 되돌려 모델이 재시도(retryable).
//! insert 는 old=기존스니펫·new=기존+추가 로 표현(별도 op 없음).

use serde_json::{json, Value};

/// Insert new Markdown without asking a model to regenerate existing content.
/// A supplied anchor must match exactly once; every old byte is preserved.
pub fn insert_content(current: &str, content: &str, after: Option<&str>) -> Result<String, String> {
    if content.trim().is_empty() {
        return Err("New content is empty".into());
    }
    let end = match after.filter(|a| !a.is_empty()) {
        Some(anchor) => {
            if current.matches(anchor).count() != 1 {
                return Err(
                    "Insertion anchor must match exactly once; read the current note again".into(),
                );
            }
            current.find(anchor).unwrap() + anchor.len()
        }
        None => current.len(),
    };
    let prefix = if end == 0 || current[..end].ends_with("\n\n") {
        ""
    } else {
        "\n\n"
    };
    let suffix = if end == current.len() || current[end..].starts_with("\n\n") {
        ""
    } else {
        "\n\n"
    };
    Ok(format!(
        "{}{prefix}{}{suffix}{}",
        &current[..end],
        content.trim(),
        &current[end..]
    ))
}

/// 렌더 시 *보이는* 텍스트만 추출(공백 정규화) — HTML 주석/스타일/스크립트/태그 제거.
/// diff 표시용 + edit 가 실제로 보이는 변화를 만들었는지 비교하는 용도.
pub fn visible_text(content: &str) -> String {
    let s = remove_block_ci(content, "<style", "</style>");
    let s = remove_block_ci(&s, "<script", "</script>");
    let s = remove_block_ci(&s, "<!--", "-->");
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// HTML 주석(`<!-- -->`)만 제거하고 **공백·줄바꿈은 보존**한다. 마크다운에선 빈 줄이
/// 문단 구분·리스트 tight/loose 를 결정하는 *의미있는* 변경이라, 공백을 뭉개면
/// 정당한 줄간격 편집이 '비가시'로 오판돼 거부된다 (Meetzy _strip_comments).
pub fn strip_comments(content: &str) -> String {
    remove_block_ci(content, "<!--", "-->")
}

/// `open`…`close` 블록을 전부 제거 (ASCII 대소문자 무시). close 가 없으면 그 지점
/// 이후를 그대로 둔다(정규식 `.*?close` 미매칭과 동일).
fn remove_block_ci(s: &str, open: &str, close: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let open_l = open.to_ascii_lowercase();
    let close_l = close.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut pos = 0usize;
    while let Some(rel) = lower[pos..].find(&open_l) {
        let start = pos + rel;
        match lower[start..].find(&close_l) {
            Some(rel_close) => {
                out.push_str(&s[pos..start]);
                pos = start + rel_close + close_l.len();
            }
            None => {
                out.push_str(&s[pos..]);
                return out;
            }
        }
    }
    out.push_str(&s[pos..]);
    out
}

/// old 를 *공백 차이를 무시*하고 content 에서 찾아 (start,end) 실제 구간들을 반환.
/// 모델이 들여쓰기·줄바꿈을 본문과 다르게(예: 2칸 vs 4칸) 복사해도 매칭되게 한다.
/// (Meetzy: old 의 연속 공백을 \s+ 로 바꾼 정규식 — 토큰 사이 공백 1개 이상 필수.)
fn whitespace_tolerant_spans(content: &str, old: &str) -> Vec<(usize, usize)> {
    let tokens: Vec<&str> = old.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut search_from = 0usize;
    'outer: while search_from <= content.len() {
        let Some(rel) = content[search_from..].find(tokens[0]) else {
            break;
        };
        let start = search_from + rel;
        let mut pos = start + tokens[0].len();
        for tok in &tokens[1..] {
            let ws_end = content[pos..]
                .find(|c: char| !c.is_whitespace())
                .map(|o| pos + o)
                .unwrap_or(content.len());
            if ws_end == pos || !content[ws_end..].starts_with(tok) {
                // 이 시작점은 실패 — 첫 토큰의 다음 문자부터 재탐색.
                search_from = start
                    + content[start..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                continue 'outer;
            }
            pos = ws_end + tok.len();
        }
        spans.push((start, pos));
        search_from = pos; // 겹침 없는 다음 매칭(regex finditer 와 동일)
    }
    spans
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// str_replace 단일 연산 적용. edits = tool args 의 JSON 배열.
/// 반환: (새 content, diffs[{before,after,before_md,after_md,count?}], errors).
/// errors 가 비어있지 않으면 호출자가 retryable 에러로 모델에 되돌린다.
pub fn apply_str_edits(content: &str, edits: &[Value]) -> (String, Vec<Value>, Vec<String>) {
    let mut new_content = content.to_string();
    let mut diffs: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for e in edits {
        let old = e.get("old").and_then(|v| v.as_str());
        let new = e.get("new").and_then(|v| v.as_str());
        let replace_all = e
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let (Some(old), Some(new)) = (old, new) else {
            errors.push("각 편집은 old(찾을 스니펫)와 new(대체 텍스트)가 필요합니다.".to_string());
            continue;
        };
        if old.is_empty() {
            errors.push("old(찾을 스니펫)가 비어 있습니다.".to_string());
            continue;
        }

        let cnt = new_content.matches(old).count();
        // replace_all=true 면 모든 출현 치환(용어/표기 일괄 변경). 기본(false)은 유일
        // 매칭 강제(동명이인 오치환 방지) — Claude Code str_replace 와 동일.
        if cnt >= 1 && replace_all {
            new_content = new_content.replace(old, new);
            diffs.push(json!({
                "before": visible_text(old), "after": visible_text(new),
                "before_md": old, "after_md": new, "count": cnt,
            }));
            continue;
        }
        if cnt == 1 {
            new_content = new_content.replacen(old, new, 1);
            diffs.push(json!({
                "before": visible_text(old), "after": visible_text(new),
                "before_md": old, "after_md": new,
            }));
            continue;
        }
        if cnt > 1 {
            errors.push(format!(
                "'{}…' 가 {cnt}곳에 있습니다. 전부 바꾸려면 replace_all=true, 한 곳만이면 주변 맥락을 더 포함해 유일하게 지정하세요.",
                truncate_chars(&visible_text(old), 40)
            ));
            continue;
        }

        // cnt == 0: 공백 관대 매칭 폴백(들여쓰기·줄바꿈 차이 흡수).
        let spans = whitespace_tolerant_spans(&new_content, old);
        if spans.len() >= 1 && replace_all {
            // 뒤에서부터 치환(앞 치환이 뒤 span 인덱스를 밀지 않게).
            for (s, t) in spans.iter().rev() {
                new_content = format!("{}{}{}", &new_content[..*s], new, &new_content[*t..]);
            }
            diffs.push(json!({
                "before": visible_text(old), "after": visible_text(new),
                "before_md": old, "after_md": new, "count": spans.len(),
            }));
            continue;
        }
        if spans.len() == 1 {
            let (s, t) = spans[0];
            new_content = format!("{}{}{}", &new_content[..s], new, &new_content[t..]);
            diffs.push(json!({
                "before": visible_text(old), "after": visible_text(new),
                "before_md": old, "after_md": new,
            }));
            continue;
        }
        if spans.len() > 1 {
            errors.push(format!(
                "'{}…' 가 {}곳에 있습니다. 전부 바꾸려면 replace_all=true, 한 곳만이면 맥락을 더 포함해 유일하게 지정하세요.",
                truncate_chars(&visible_text(old), 40),
                spans.len()
            ));
            continue;
        }
        errors.push(format!(
            "본문에서 찾지 못함: '{}…'. (직전 편집으로 본문이 이미 바뀌었을 수 있음 — read_minutes로 *현재* 본문을 다시 확인한 뒤, 거기 있는 그대로 지정하세요.)",
            truncate_chars(&visible_text(old), 40)
        ));
    }
    (new_content, diffs, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_preserves_other_sections_and_rejects_ambiguous_anchor() {
        let before = "# 계획\n\n## A\n- 조건부 배포\n\n## B\n- 금액 130만원";
        let inserted = insert_content(before, "- 보안 검토 필요", Some("- 조건부 배포")).unwrap();
        assert_eq!(inserted.replace("\n\n- 보안 검토 필요", ""), before);
        assert!(insert_content("반복 반복", "새 내용", Some("반복")).is_err());
        assert_eq!(insert_content("", "# 시작", None).unwrap(), "# 시작");
        assert!(insert_content(before, " ", None).is_err());
    }

    #[test]
    fn unique_replace() {
        let (out, diffs, errs) = apply_str_edits(
            "# 제목\n\n- 항목 하나\n- 항목 둘\n",
            &[json!({"old": "항목 하나", "new": "항목 1"})],
        );
        assert!(errs.is_empty());
        assert_eq!(diffs.len(), 1);
        assert!(out.contains("- 항목 1\n"));
    }

    #[test]
    fn ambiguous_without_replace_all() {
        let (_, _, errs) = apply_str_edits("aaa bbb aaa", &[json!({"old": "aaa", "new": "ccc"})]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("replace_all"));
    }

    #[test]
    fn replace_all_counts() {
        let (out, diffs, errs) = apply_str_edits(
            "김상무 발언. 김상무 결정.",
            &[json!({"old": "김상무", "new": "김상우", "replace_all": true})],
        );
        assert!(errs.is_empty());
        assert_eq!(diffs[0]["count"], json!(2));
        assert!(!out.contains("김상무"));
    }

    #[test]
    fn whitespace_tolerant_fallback() {
        // 본문은 2칸 들여쓰기, 모델은 4칸으로 복사 — 폴백 매칭.
        let content = "- a\n  - nested item\n- b\n";
        let (out, diffs, errs) = apply_str_edits(
            content,
            &[json!({"old": "- a\n    - nested item", "new": "- a\n  - changed"})],
        );
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(diffs.len(), 1);
        assert!(out.contains("- changed"));
    }

    #[test]
    fn not_found() {
        let (_, _, errs) = apply_str_edits("본문", &[json!({"old": "없는말", "new": "x"})]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("찾지 못함"));
    }

    #[test]
    fn comment_only_guard_helpers() {
        // 주석만 추가(제거 후 공백까지 동일) → comment-only 로 판정돼 거부된다.
        let a = "line1\n\nline2";
        let b = "line1\n<!-- note -->\nline2";
        assert_eq!(strip_comments(a), strip_comments(b));
        // 빈 줄 추가는 주석 제거 후에도 다름 → 유효한 변경(마크다운 줄간격 편집 허용).
        let c = "line1\nline2";
        assert_ne!(strip_comments(a), strip_comments(c));
        assert_eq!(strip_comments("line1<!-- x -->line2"), "line1line2");
    }
}
