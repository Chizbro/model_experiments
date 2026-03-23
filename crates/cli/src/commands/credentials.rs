use crate::api_client::ApiClient;
use anyhow::Result;
use api_types::UpdateIdentityRequest;

pub async fn show(client: &ApiClient) -> Result<()> {
    let resp = client.get_identity("default").await?;

    println!("Credentials (identity: default):");
    println!(
        "  Git token:   {}",
        if resp.has_git_token {
            "configured"
        } else {
            "not set"
        }
    );
    println!(
        "  Agent token: {}",
        if resp.has_agent_token {
            "configured"
        } else {
            "not set"
        }
    );

    if !resp.has_git_token || !resp.has_agent_token {
        eprintln!(
            "\nHint: Both git and agent tokens are required to run sessions. Use `remote-harness credentials set` to configure them."
        );
    }

    Ok(())
}

pub async fn set(
    client: &ApiClient,
    git_token: Option<&str>,
    agent_token: Option<&str>,
) -> Result<()> {
    // If neither flag is provided, prompt interactively
    let git_token_val = match git_token {
        Some(t) => Some(t.to_string()),
        None => {
            eprint!("Git token (leave empty to skip): ");
            let token = rpassword::read_password().map_err(|e| anyhow::anyhow!("Failed to read git token: {}", e))?;
            if token.is_empty() {
                None
            } else {
                Some(token)
            }
        }
    };

    let agent_token_val = match agent_token {
        Some(t) => Some(t.to_string()),
        None => {
            eprint!("Agent token (leave empty to skip): ");
            let token = rpassword::read_password().map_err(|e| anyhow::anyhow!("Failed to read agent token: {}", e))?;
            if token.is_empty() {
                None
            } else {
                Some(token)
            }
        }
    };

    if git_token_val.is_none() && agent_token_val.is_none() {
        println!("No tokens provided. Nothing to update.");
        return Ok(());
    }

    let req = UpdateIdentityRequest {
        git_token: git_token_val,
        agent_token: agent_token_val,
        refresh_token: None,
    };

    client.update_identity("default", &req).await?;
    println!("saved");
    Ok(())
}
