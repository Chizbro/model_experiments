use crate::api_client::ApiClient;
use anyhow::Result;

pub async fn run(client: &ApiClient) -> Result<()> {
    let resp = client.health().await?;
    println!("{}", resp.status);
    Ok(())
}
