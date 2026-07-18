//! Insights & stats endpoint.

use better_im_index::InsightsData;
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

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

impl From<InsightsData> for InsightsDto {
    fn from(d: InsightsData) -> Self {
        Self {
            total_messages: d.total_messages,
            sent_count: d.sent_count,
            received_count: d.received_count,
            first_message: d.first_message.map(|t| t.to_rfc3339()),
            last_message: d.last_message.map(|t| t.to_rfc3339()),
            by_day: d
                .by_day
                .into_iter()
                .map(|c| DayCountDto {
                    date: c.date,
                    count: c.count,
                })
                .collect(),
            by_hour: d
                .by_hour
                .into_iter()
                .map(|c| HourCountDto {
                    hour: c.hour,
                    count: c.count,
                })
                .collect(),
            top_contacts: d
                .top_contacts
                .into_iter()
                .map(|c| ContactCountDto {
                    handle: c.handle,
                    count: c.count,
                })
                .collect(),
        }
    }
}

/// Compute conversation / global insights by aggregating over the index. No Full
/// Disk Access needed — everything comes from the local index.
#[tauri::command]
pub async fn get_insights(
    state: State<'_, AppState>,
    chat_id: Option<i64>,
) -> Result<InsightsDto, String> {
    let indexer = state.indexer.clone();
    super::run_blocking(move || {
        let guard = indexer.lock().map_err(|e| e.to_string())?;
        let data = guard
            .db()
            .insights(chat_id)
            .map_err(|e| format!("{e:#}"))?;
        Ok(InsightsDto::from(data))
    })
    .await
}
