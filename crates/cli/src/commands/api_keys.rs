use crate::api_client::ApiClient;
use anyhow::Result;

/// `api-key create [--label <label>]` — Create a new API key.
pub async fn create(client: &ApiClient, label: Option<&str>) -> Result<()> {
    let resp = client.create_api_key(label).await?;

    println!("API key created successfully.");
    println!();
    println!("  ID:      {}", resp.id);
    if let Some(ref l) = resp.label {
        println!("  Label:   {}", l);
    }
    println!("  Created: {}", resp.created_at);
    println!();
    println!("  Key: {}", resp.key);
    println!();
    println!("  WARNING: This key will not be shown again. Store it securely.");

    Ok(())
}

/// `api-key list` — List API keys.
pub async fn list(client: &ApiClient) -> Result<()> {
    let resp = client.list_api_keys().await?;

    if resp.items.is_empty() {
        println!("No API keys found.");
        return Ok(());
    }

    let header_created = "CREATED";
    println!("{:<38} {:<20} {}", "ID", "LABEL", header_created);
    println!("{}", "-".repeat(80));

    for key in &resp.items {
        let label = key.label.as_deref().unwrap_or("-");
        println!(
            "{:<38} {:<20} {}",
            key.id,
            label,
            key.created_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    if resp.next_cursor.is_some() {
        println!("\n(more keys exist; use pagination to see all)");
    }

    Ok(())
}

/// `api-key revoke <id>` — Revoke an API key.
pub async fn revoke(client: &ApiClient, id: &str) -> Result<()> {
    client.revoke_api_key(id).await?;
    println!("API key {} revoked.", id);
    Ok(())
}
