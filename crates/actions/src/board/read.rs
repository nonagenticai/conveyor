#![forbid(unsafe_code)]
//! The card read model's satellites.
//!
//! `cards.get` and `GET /api/cards/{id}` return the SAME document, so the assembly lives
//! here once: the card row, its labels, its newest comments, its checklists with their items
//! and its custom-field values, all read inside the caller's transaction — one consistent
//! snapshot, never a per-satellite request from the client (S05 §5).
//!
//! Nothing here decides anything: it reads and maps. Every query is the store's.

use std::collections::HashMap;

use conveyor_store as store;
use conveyor_store::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

use super::views::{
    CardView, card_out, comment_out, custom_field_value_out, label_out, task_list_out,
};

/// Read one card and its satellites, or `None` when the card is not in this tenant.
pub async fn card_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CardView>, sqlx::Error> {
    let Some(record) = store::card::get_card_record_tx(&mut *conn, tenant, id).await? else {
        return Ok(None);
    };
    let labels = store::label::list_card_labels_tx(&mut *conn, tenant, id).await?;
    let comments = store::comment::card_view_comments_tx(&mut *conn, tenant, id).await?;
    let lists = store::task::list_task_lists_tx(&mut *conn, tenant, id).await?;
    let tasks = store::task::list_tasks_for_card_tx(&mut *conn, tenant, id).await?;
    let values = store::custom_field::list_values_tx(&mut *conn, tenant, id).await?;

    let mut by_list: HashMap<Uuid, Vec<store::task::TaskRecord>> = HashMap::new();
    for task in tasks {
        by_list.entry(task.task_list_id).or_default().push(task);
    }
    let task_lists = lists
        .iter()
        .map(|list| {
            let empty = Vec::new();
            let items = by_list.get(&list.id).unwrap_or(&empty);
            task_list_out(list, items)
        })
        .collect();

    Ok(Some(CardView {
        card: card_out(&record),
        labels: labels.iter().map(label_out).collect(),
        comments: comments.iter().map(comment_out).collect(),
        task_lists,
        custom_field_values: values.iter().map(custom_field_value_out).collect(),
    }))
}
