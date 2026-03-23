use crate::api_client::ApiClient;
use anyhow::Result;

pub async fn list(client: &ApiClient) -> Result<()> {
    let resp = client.list_workers().await?;

    if resp.items.is_empty() {
        println!("No workers found.");
        return Ok(());
    }

    println!(
        "{:<38} {:<20} {:<10} {:<22} LABELS",
        "WORKER ID", "HOST", "STATUS", "LAST SEEN"
    );
    println!("{}", "-".repeat(110));

    for item in &resp.items {
        let labels_str = if item.labels.is_empty() {
            String::new()
        } else {
            item.labels
                .iter()
                .map(|(k, v)| {
                    if v.is_string() {
                        format!("{}={}", k, v.as_str().unwrap_or(""))
                    } else {
                        format!("{}={}", k, v)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        println!(
            "{:<38} {:<20} {:<10} {:<22} {}",
            item.worker_id,
            item.host,
            item.status,
            item.last_seen_at.format("%Y-%m-%d %H:%M:%S"),
            labels_str
        );
    }

    if resp.next_cursor.is_some() {
        println!("\n(more workers available)");
    }

    Ok(())
}

pub async fn clear(client: &ApiClient, id: &str) -> Result<()> {
    client.delete_worker(id).await?;
    println!("removed");
    Ok(())
}
