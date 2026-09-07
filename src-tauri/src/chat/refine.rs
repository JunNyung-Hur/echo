//! Deterministic insertion and version persistence for freeform notes.
//! The conversation agent writes the new content; there is no second writer
//! model and no whole-note regeneration on append.

use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{Note, NoteBody};
use crate::repo::{note_bodies, notes};
use uuid::Uuid;

pub(crate) fn extract_title(content: &str) -> String {
    if let Some(t) = extract_md_h1(content) {
        return t;
    }
    extract_title_html(content)
}

/// 첫 `# ` 헤딩(코드펜스 밖)에서 제목 텍스트를 뽑는다. `## ` 이상 섹션은 제외.
/// `[링크](url)` → 링크 텍스트, `*`/`_`/`` ` `` 인라인 마크 제거.
fn extract_md_h1(content: &str) -> Option<String> {
    let mut in_fence = false;
    for line in content.lines() {
        let lt = line.trim_start();
        if lt.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = lt.strip_prefix("# ") {
            let mut text = strip_md_links(rest.trim().trim_end_matches('#').trim());
            text.retain(|c| !matches!(c, '*' | '_' | '`'));
            let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !squashed.is_empty() {
                return Some(squashed.chars().take(120).collect());
            }
        }
    }
    None
}

/// `[text](url)` → `text` (마크다운 링크의 라벨만 남김).
fn strip_md_links(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(close) = chars[i + 1..]
                .iter()
                .position(|&c| c == ']')
                .map(|p| p + i + 1)
            {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = chars[close + 2..]
                        .iter()
                        .position(|&c| c == ')')
                        .map(|p| p + close + 2)
                    {
                        out.extend(&chars[i + 1..close]);
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 레거시 HTML 본문의 첫 의미있는 텍스트 줄(태그 제거).
fn extract_title_html(html: &str) -> String {
    // full HTML 문서(minutes)면 <body> 이후만 본다 — <head>/<style> 안의 CSS가
    // 첫 줄로 뽑혀 "body {" 같은 제목이 되는 걸 막는다. 콘텐츠 조각(freeform)은 그대로.
    let scope = match html.find("<body") {
        Some(idx) => {
            let after = &html[idx..];
            after.find('>').map(|g| &after[g + 1..]).unwrap_or(html)
        }
        None => html,
    };
    let mut text = String::new();
    let mut in_tag = false;
    for c in scope.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push('\n');
            }
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(120).collect::<String>())
        .unwrap_or_else(|| "제목 없음".to_string())
}

pub async fn run_insert(
    pool: &DbPool,
    note_id: &str,
    content: &str,
    after: Option<&str>,
    base_version: Option<&str>,
) -> Result<String> {
    let note = notes::get(pool, note_id).await?;
    if note.note_type.as_deref() != Some("freeform") {
        return Err(Error::InvalidInput(
            "New content insertion is only available for freeform notes".into(),
        ));
    }
    let active = note_bodies::get_active(pool, note_id).await?;
    if (after.is_some() && base_version.is_none())
        || base_version.is_some_and(|v| active.as_ref().map(|b| b.id.as_str()) != Some(v))
    {
        return Err(Error::Other(
            "The note changed or its version is missing. Read the current note before inserting."
                .into(),
        ));
    }
    let current = match &active {
        Some(b) => {
            let path = b
                .content_path
                .as_ref()
                .ok_or_else(|| Error::Other("Current note file missing".into()))?;
            tokio::fs::read_to_string(crate::storage::resolve(path)).await?
        }
        None => String::new(),
    };
    if current.trim_start().starts_with('<') {
        return Err(Error::Other("This is a legacy HTML note. Use read_minutes/edit_minutes to convert it to Markdown before inserting.".into()));
    }
    let updated =
        super::edit::insert_content(&current, content, after).map_err(Error::InvalidInput)?;
    persist_body(pool, note_id, &note, &active, &updated).await
}

async fn persist_body(
    pool: &DbPool,
    note_id: &str,
    note: &Note,
    active: &Option<NoteBody>,
    html: &str,
) -> Result<String> {
    // Note-centric storage — body under the note's folder, stored app_data-relative.
    let new_id = Uuid::new_v4().to_string();
    let path_str = crate::storage::body_rel(note_id, &new_id, crate::storage::body_ext_for(html));
    let path = crate::storage::resolve(&path_str);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, html.as_bytes()).await?;

    let ctx = crate::worker::generate::context_snapshot_json(note);

    // 기존 버전이 있으면 archive 후 새 완료 버전(이력 누적). G-VERSION 베이스라인 유지.
    let (initial_content, initial_ctx) = match active {
        Some(b) => (
            b.initial_content_path
                .clone()
                .or_else(|| b.content_path.clone()),
            b.initial_context_snapshot
                .clone()
                .or_else(|| Some(b.context_snapshot.clone())),
        ),
        None => (None, None),
    };
    let transcript_id = active.as_ref().and_then(|b| b.transcript_id.as_deref());

    let saved = note_bodies::archive_and_create_completed(
        pool,
        &new_id,
        note_id,
        transcript_id,
        &path_str,
        &ctx,
        initial_content.as_deref(),
        initial_ctx.as_deref(),
        false,
        None,
        active.as_ref().map(|b| b.id.as_str()),
    )
    .await;
    if let Err(e) = saved {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(e);
    }

    // 본문 첫 줄을 노트 제목으로 동기화 — 제목은 본문에서 도출(read-only 메타).
    // archive 이후에 실행한다: 제목 변경이 폴더 리네임을 유발하면 방금 insert된
    // 본문 행의 경로 prefix까지 rewrite_paths가 함께 갱신해 일관성이 유지된다.
    let _ = notes::update(
        pool,
        note_id,
        notes::UpdateNoteInput {
            title: Some(extract_title(html)),
            ..Default::default()
        },
    )
    .await;

    Ok(new_id)
}
