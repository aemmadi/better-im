//! Insights & stats endpoint.

use serde::Serialize;

/// Messages sent on a given calendar day (`YYYY-MM-DD`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayCountDto {
    pub date: String,
    pub count: i64,
}

/// Messages sent in a given hour of day (`0..=23`, local time).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourCountDto {
    pub hour: i32,
    pub count: i64,
}

/// A correspondent ranked by message volume. `handle` is the raw phone/email;
/// the frontend resolves it to a name via the existing `resolve_contacts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactCountDto {
    pub handle: String,
    pub count: i64,
}

/// Aggregate stats for one conversation (`chat_id = Some`) or all of them (`None`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsDto {
    pub total_messages: i64,
    pub sent_count: i64,
    pub received_count: i64,
    pub first_message: Option<String>,
    pub last_message: Option<String>,
    pub by_day: Vec<DayCountDto>,
    pub by_hour: Vec<HourCountDto>,
    pub top_contacts: Vec<ContactCountDto>,
}

/// Compute conversation / global insights.
///
/// TODO(phase4-insights): implement with aggregate SQL over the index
/// (`GROUP BY` day/hour/handle, counts, min/max timestamp). Reuse the Phase 3
/// contact index server-side if convenient, else let the frontend resolve names.
#[tauri::command]
pub async fn get_insights(chat_id: Option<i64>) -> Result<InsightsDto, String> {
    let _ = chat_id;
    Ok(InsightsDto {
        total_messages: 0,
        sent_count: 0,
        received_count: 0,
        first_message: None,
        last_message: None,
        by_day: Vec::new(),
        by_hour: Vec::new(),
        top_contacts: Vec::new(),
    })
}
