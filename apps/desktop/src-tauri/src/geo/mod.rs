pub mod commands;

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Nominatim response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
    display_name: String,
}

// ---------------------------------------------------------------------------
// Job definition
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum GeoJob {
    GeocodeEntity { entity_id: String },
    GeocodeItemEntities { item_id: String },
}

// ---------------------------------------------------------------------------
// Event payloads
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct GeoCompletePayload {
    pub entity_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub display_name: String,
}

#[derive(Clone, Serialize)]
pub struct GeoItemCompletePayload {
    pub item_id: String,
    pub geocoded_count: usize,
    pub not_found_count: usize,
}

#[derive(Clone, Serialize)]
pub struct GeoErrorPayload {
    pub id: String,
    pub error: String,
}

fn emit_entity_complete(app_handle: &AppHandle, payload: &GeoCompletePayload) {
    let _ = app_handle.emit("geo:entity-complete", payload.clone());
}

fn emit_item_complete(app_handle: &AppHandle, payload: &GeoItemCompletePayload) {
    let _ = app_handle.emit("geo:item-complete", payload.clone());
}

fn emit_error(app_handle: &AppHandle, id: &str, error: &str) {
    let _ = app_handle.emit(
        "geo:error",
        GeoErrorPayload {
            id: id.to_string(),
            error: error.to_string(),
        },
    );
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

pub struct GeoQueue {
    sender: mpsc::Sender<GeoJob>,
}

impl GeoQueue {
    pub fn new() -> (Self, mpsc::Receiver<GeoJob>) {
        let (sender, receiver) = mpsc::channel::<GeoJob>(64);
        (Self { sender }, receiver)
    }

    pub fn submit(&self, job: GeoJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("Geo queue full or closed: {e}"))
    }

    pub fn start_worker(
        db_path: PathBuf,
        mut receiver: mpsc::Receiver<GeoJob>,
        app_handle: AppHandle,
    ) {
        tauri::async_runtime::spawn(async move {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let _ =
                        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");
                    c
                }
                Err(e) => {
                    eprintln!("[geo] Failed to open worker DB connection: {e}");
                    return;
                }
            };

            let client = reqwest::Client::builder()
                .user_agent("EntropIA-Desktop/0.1 (historical-research-app)")
                .build()
                .unwrap();

            eprintln!("[geo] Worker ready — Nominatim geocoding enabled.");

            while let Some(job) = receiver.recv().await {
                match job {
                    GeoJob::GeocodeEntity { entity_id } => {
                        let result = geocode_single_entity(&conn, &client, &entity_id).await;
                        match result {
                            Ok(Some(payload)) => emit_entity_complete(&app_handle, &payload),
                            Ok(None) => {} // not_found, already updated in DB
                            Err(e) => emit_error(&app_handle, &entity_id, &e),
                        }
                    }
                    GeoJob::GeocodeItemEntities { item_id } => {
                        let result =
                            geocode_all_place_entities(&conn, &client, &item_id, &app_handle)
                                .await;
                        match result {
                            Ok(payload) => emit_item_complete(&app_handle, &payload),
                            Err(e) => emit_error(&app_handle, &item_id, &e),
                        }
                    }
                }
            }

            eprintln!("[geo] Worker loop ended — channel closed.");
        });
    }
}

// ---------------------------------------------------------------------------
// Geocoding logic
// ---------------------------------------------------------------------------

async fn nominatim_search(
    client: &reqwest::Client,
    query: &str,
) -> Result<Option<NominatimResult>, String> {
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1",
        urlencoding::encode(query)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Nominatim request failed: {e}"))?;

    let results: Vec<NominatimResult> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Nominatim response: {e}"))?;

    Ok(results.into_iter().next())
}

async fn geocode_single_entity(
    conn: &rusqlite::Connection,
    client: &reqwest::Client,
    entity_id: &str,
) -> Result<Option<GeoCompletePayload>, String> {
    // Fetch entity value and type
    let (value, entity_type): (String, String) = conn
        .query_row(
            "SELECT value, entity_type FROM entities WHERE id = ?1",
            params![entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Entity not found: {e}"))?;

    // Only geocode place-type entities
    if entity_type != "place" {
        conn.execute(
            "UPDATE entities SET geo_status = 'skipped' WHERE id = ?1",
            params![entity_id],
        )
        .ok();
        return Ok(None);
    }

    match nominatim_search(client, &value).await? {
        Some(result) => {
            let lat: f64 = result
                .lat
                .parse()
                .map_err(|_| "Invalid latitude from Nominatim".to_string())?;
            let lon: f64 = result
                .lon
                .parse()
                .map_err(|_| "Invalid longitude from Nominatim".to_string())?;

            conn.execute(
                "UPDATE entities SET latitude = ?1, longitude = ?2, geo_status = 'resolved' WHERE id = ?3",
                params![lat, lon, entity_id],
            )
            .map_err(|e| format!("Failed to update entity coordinates: {e}"))?;

            eprintln!("[geo] Resolved: '{}' → ({}, {})", value, lat, lon);

            Ok(Some(GeoCompletePayload {
                entity_id: entity_id.to_string(),
                latitude: lat,
                longitude: lon,
                display_name: result.display_name,
            }))
        }
        None => {
            conn.execute(
                "UPDATE entities SET geo_status = 'not_found' WHERE id = ?1",
                params![entity_id],
            )
            .ok();
            eprintln!("[geo] Not found: '{}'", value);
            Ok(None)
        }
    }
}

async fn geocode_all_place_entities(
    conn: &rusqlite::Connection,
    client: &reqwest::Client,
    item_id: &str,
    app_handle: &AppHandle,
) -> Result<GeoItemCompletePayload, String> {
    // Find all place entities for this item that haven't been geocoded yet
    let mut stmt = conn
        .prepare(
            "SELECT id FROM entities
             WHERE item_id = ?1
               AND entity_type = 'place'
               AND geo_status = 'pending'
               AND (source IS NULL OR source != 'manual_deleted')",
        )
        .map_err(|e| format!("Failed to query entities: {e}"))?;

    let entity_ids: Vec<String> = stmt
        .query_map(params![item_id], |row| row.get(0))
        .map_err(|e| format!("Failed to fetch entities: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let total = entity_ids.len();
    eprintln!("[geo] Geocoding {total} place entities for item {item_id}");

    let mut geocoded_count = 0;
    let mut not_found_count = 0;

    for (i, entity_id) in entity_ids.iter().enumerate() {
        // Rate limit: 1 request per second (Nominatim policy)
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(1100)).await;
        }

        match geocode_single_entity(conn, client, entity_id).await {
            Ok(Some(payload)) => {
                geocoded_count += 1;
                emit_entity_complete(app_handle, &payload);
            }
            Ok(None) => {
                not_found_count += 1;
            }
            Err(e) => {
                eprintln!("[geo] Error geocoding entity {entity_id}: {e}");
                emit_error(app_handle, entity_id, &e);
            }
        }
    }

    Ok(GeoItemCompletePayload {
        item_id: item_id.to_string(),
        geocoded_count,
        not_found_count,
    })
}

/// Auto-trigger: call this after NER completes for an item.
pub fn enqueue_geocoding_for_item(geo_queue: &GeoQueue, item_id: &str) -> Result<(), String> {
    geo_queue.submit(GeoJob::GeocodeItemEntities {
        item_id: item_id.to_string(),
    })
}
