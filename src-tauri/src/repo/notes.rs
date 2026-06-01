//! notes repository.

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::{Note, NoteListItem, Tag};

// ============================================================================
// Inputs
// ============================================================================

#[derive(Debug, Default, Deserialize)]
pub struct CreateNoteInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    /// auto / kor / eng
    pub language: Option<String>,
    /// ISO-8601 string.
    pub started_at: Option<String>,
    /// "minutes" | "freeform" — 유형 선택 시 생성 시점에 함께 저장. null이면 진입 시 선택.
    pub note_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateNoteInput {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub location: Option<Option<String>>,
    pub language: Option<String>,
    pub started_at: Option<Option<String>>,
    /// "minutes" | "freeform" — 유형 선택 시 한 번 설정(이후 고정).
    pub note_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListNotesQuery {
    /// Substring (case-insensitive) over title/description/location.
    pub q: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    /// #tag tokens parsed from the search box — each must be present (AND).
    #[serde(default)]
    pub tag_names: Vec<String>,
    /// 1-based page number.
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    30
}

#[derive(Debug, Serialize)]
pub struct ListNotesResponse {
    pub items: Vec<NoteListItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

// ============================================================================
// Operations
// ============================================================================

/// Create a new note. Title defaults to `내 노트-YYMMDD-HHMM` if blank
/// (F-NOTE-001 — old `meeting_service.create_meeting`).
pub async fn create(pool: &SqlitePool, input: CreateNoteInput) -> Result<Note> {
    let id = Uuid::new_v4().to_string();
    // 4a3b683 — 빈 제목 기본값은 ui_lang(설정)에 맞춤(내 노트 / My Note).
    let ui_lang = crate::repo::settings::get(pool, "ui_lang").await.ok().flatten();
    let locale = if ui_lang.as_deref() == Some("en") { "en" } else { "ko" };
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
        .unwrap_or_else(|| default_title(locale));
    let language = input
        .language
        .as_deref()
        .filter(|l| matches!(*l, "auto" | "kor" | "eng"))
        .map(String::from)
        .unwrap_or_else(|| "auto".to_string());

    sqlx::query(
        "INSERT INTO notes (id, title, description, location, language, started_at, note_type) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&title)
    .bind(&input.description)
    .bind(&input.location)
    .bind(&language)
    .bind(&input.started_at)
    .bind(&input.note_type)
    .execute(pool)
    .await?;

    get(pool, &id).await
}

fn default_title(locale: &str) -> String {
    // 빈 노트 제목은 본문 첫 줄에서 자동 추출되므로, 본문이 비어 있는 동안의
    // 표시 제목만 담당한다(extract_title의 빈 본문 fallback과 동일).
    if locale == "en" {
        "Untitled".to_string()
    } else {
        "제목 없음".to_string()
    }
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Note> {
    sqlx::query_as::<_, Note>("SELECT * FROM notes WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("note {id}")))
}

pub async fn update(pool: &SqlitePool, id: &str, input: UpdateNoteInput) -> Result<Note> {
    // Ensure exists first (clean 404 instead of silent no-op).
    let _existing = get(pool, id).await?;

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE notes SET ");
    let mut first = true;
    let add = |qb: &mut QueryBuilder<Sqlite>, first: &mut bool, col: &str| {
        if !*first {
            qb.push(", ");
        }
        qb.push(col);
        qb.push(" = ");
        *first = false;
    };

    if let Some(v) = &input.title {
        add(&mut qb, &mut first, "title");
        qb.push_bind(v);
    }
    if let Some(v) = &input.description {
        add(&mut qb, &mut first, "description");
        qb.push_bind(v);
    }
    if let Some(v) = &input.location {
        add(&mut qb, &mut first, "location");
        qb.push_bind(v);
    }
    if let Some(v) = &input.language {
        if !matches!(v.as_str(), "auto" | "kor" | "eng") {
            return Err(Error::InvalidInput(format!("language: {v}")));
        }
        add(&mut qb, &mut first, "language");
        qb.push_bind(v);
    }
    if let Some(v) = &input.started_at {
        add(&mut qb, &mut first, "started_at");
        qb.push_bind(v);
    }
    if let Some(v) = &input.note_type {
        add(&mut qb, &mut first, "note_type");
        qb.push_bind(v);
    }

    if first {
        // No fields to change — return current row.
        return get(pool, id).await;
    }

    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(pool).await?;
    get(pool, id).await
}

/// Delete a note. ON DELETE CASCADE handles recordings / transcripts /
/// note_bodies / chat / timeline / tags (G-DB-002).
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    let res = sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound(format!("note {id}")));
    }
    Ok(())
}

/// Paginated list with search + date filter + `has_active_task` flag.
/// Mirrors `meeting_service.get_meetings()` (F-LIST-001..008).
pub async fn list(pool: &SqlitePool, q: ListNotesQuery) -> Result<ListNotesResponse> {
    let page = q.page.max(1);
    let page_size = q.page_size.clamp(1, 200);
    let offset = (page - 1) * page_size;

    // ---- WHERE builder (shared between count and select) ----
    let mut where_qb: QueryBuilder<Sqlite> = QueryBuilder::new(" WHERE 1=1");
    if let Some(needle_raw) = q.q.as_deref().map(str::trim).filter(|s| s.len() >= 2) {
        let needle = format!("%{}%", needle_raw.replace('%', "\\%").replace('_', "\\_"));
        where_qb.push(" AND (n.title LIKE ");
        where_qb.push_bind(needle.clone());
        where_qb.push(" ESCAPE '\\' OR COALESCE(n.description,'') LIKE ");
        where_qb.push_bind(needle.clone());
        where_qb.push(" ESCAPE '\\' OR COALESCE(n.location,'') LIKE ");
        where_qb.push_bind(needle);
        where_qb.push(" ESCAPE '\\')");
    }
    if let Some(from) = q.from_date.as_deref().filter(|s| !s.is_empty()) {
        where_qb.push(" AND COALESCE(n.started_at, n.created_at) >= ");
        where_qb.push_bind(format!("{from} 00:00:00"));
    }
    if let Some(to) = q.to_date.as_deref().filter(|s| !s.is_empty()) {
        where_qb.push(" AND COALESCE(n.started_at, n.created_at) <= ");
        where_qb.push_bind(format!("{to} 23:59:59"));
    }

    // ---- count ----
    // Rebuild a fresh builder because QueryBuilder is single-use.
    let mut count_qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT COUNT(*) FROM notes n");
    append_where(
        &mut count_qb,
        q.q.as_deref(),
        q.from_date.as_deref(),
        q.to_date.as_deref(),
        &q.tag_names,
    );
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    // ---- select ----
    let mut sel_qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        r#"SELECT
            n.id,
            n.title,
            n.description,
            n.location,
            n.started_at,
            n.note_type,
            n.created_at,
            n.updated_at,
            CASE WHEN EXISTS (
                    SELECT 1 FROM transcripts t
                    WHERE t.note_id = n.id AND t.status IN ('pending','processing')
                 ) OR EXISTS (
                    SELECT 1 FROM note_bodies b
                    WHERE b.note_id = n.id AND b.status IN ('pending','processing')
                 ) THEN 1 ELSE 0 END AS has_active_task
         FROM notes n"#,
    );
    append_where(
        &mut sel_qb,
        q.q.as_deref(),
        q.from_date.as_deref(),
        q.to_date.as_deref(),
        &q.tag_names,
    );
    sel_qb.push(" ORDER BY COALESCE(n.started_at, n.created_at) DESC, n.id DESC");
    sel_qb.push(" LIMIT ").push_bind(page_size);
    sel_qb.push(" OFFSET ").push_bind(offset);

    let mut items: Vec<NoteListItem> = sel_qb
        .build_query_as::<NoteListItem>()
        .fetch_all(pool)
        .await?;

    attach_tags(pool, &mut items).await?;

    Ok(ListNotesResponse {
        items,
        total,
        page,
        page_size,
    })
}

/// Fill each list item's `tags` in a single round-trip (avoids N+1 over the
/// page). Tags are name-sorted within each note.
async fn attach_tags(pool: &SqlitePool, items: &mut [NoteListItem]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT nt.note_id, t.id, t.name, t.color, t.created_at \
         FROM note_tags nt JOIN tags t ON t.id = nt.tag_id WHERE nt.note_id IN (",
    );
    let mut sep = qb.separated(", ");
    for item in items.iter() {
        sep.push_bind(item.id.clone());
    }
    qb.push(") ORDER BY nt.rowid");

    let rows: Vec<(String, String, String, Option<String>, String)> =
        qb.build_query_as().fetch_all(pool).await?;

    let mut by_note: std::collections::HashMap<String, Vec<Tag>> = std::collections::HashMap::new();
    for (note_id, id, name, color, created_at) in rows {
        by_note.entry(note_id).or_default().push(Tag {
            id,
            name,
            color,
            created_at,
        });
    }
    for item in items.iter_mut() {
        if let Some(tags) = by_note.remove(&item.id) {
            item.tags = tags;
        }
    }
    Ok(())
}

/// Helper for `list` so count and select share identical WHERE.
fn append_where(
    qb: &mut QueryBuilder<Sqlite>,
    q: Option<&str>,
    from_date: Option<&str>,
    to_date: Option<&str>,
    tag_names: &[String],
) {
    qb.push(" WHERE 1=1");
    // Each #tag token must be present (AND); prefix match so "#회" finds "회의".
    for tag in tag_names.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let pat = format!(
            "{}%",
            tag.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        );
        qb.push(" AND EXISTS (SELECT 1 FROM note_tags nt JOIN tags t ON t.id = nt.tag_id WHERE nt.note_id = n.id AND t.name LIKE ");
        qb.push_bind(pat);
        qb.push(" ESCAPE '\\' COLLATE NOCASE)");
    }
    if let Some(needle_raw) = q.map(str::trim).filter(|s| s.len() >= 2) {
        let needle = format!("%{}%", needle_raw.replace('%', "\\%").replace('_', "\\_"));
        qb.push(" AND (n.title LIKE ");
        qb.push_bind(needle.clone());
        qb.push(" ESCAPE '\\' OR COALESCE(n.description,'') LIKE ");
        qb.push_bind(needle.clone());
        qb.push(" ESCAPE '\\' OR COALESCE(n.location,'') LIKE ");
        qb.push_bind(needle);
        qb.push(" ESCAPE '\\')");
    }
    if let Some(from) = from_date.filter(|s| !s.is_empty()) {
        qb.push(" AND COALESCE(n.started_at, n.created_at) >= ");
        qb.push_bind(format!("{from} 00:00:00"));
    }
    if let Some(to) = to_date.filter(|s| !s.is_empty()) {
        qb.push(" AND COALESCE(n.started_at, n.created_at) <= ");
        qb.push_bind(format!("{to} 23:59:59"));
    }
}
