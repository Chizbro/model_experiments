use crate::api_client::ApiClient;
use anyhow::{bail, Result};
use api_types::*;

pub struct StartParams<'a> {
    pub repo: &'a str,
    pub workflow: &'a str,
    pub prompt: Option<&'a str>,
    pub agent_cli: Option<&'a str>,
    pub n: Option<u32>,
    pub sentinel: Option<&'a str>,
    pub branch_mode: Option<&'a str>,
    pub ref_name: Option<&'a str>,
    pub persona_id: Option<&'a str>,
    pub model: Option<&'a str>,
}

pub async fn start(client: &ApiClient, params: StartParams<'_>) -> Result<()> {
    let StartParams {
        repo,
        workflow,
        prompt,
        agent_cli,
        n,
        sentinel,
        branch_mode,
        ref_name,
        persona_id,
        model,
    } = params;
    // Parse workflow type
    let wf: WorkflowType = serde_json::from_str(&format!("\"{}\"", workflow))
        .map_err(|_| anyhow::anyhow!("Invalid workflow type: {}. Use: chat, loop_n, loop_until_sentinel, inbox", workflow))?;

    // Build params based on workflow type
    let mut params = serde_json::Map::new();

    match wf {
        WorkflowType::Chat => {
            if let Some(p) = prompt {
                params.insert("prompt".to_string(), serde_json::Value::String(p.to_string()));
            } else {
                bail!("--prompt is required for chat workflow");
            }
        }
        WorkflowType::LoopN => {
            if let Some(p) = prompt {
                params.insert("prompt".to_string(), serde_json::Value::String(p.to_string()));
            } else {
                bail!("--prompt is required for loop_n workflow");
            }
            if let Some(count) = n {
                params.insert(
                    "n".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(count)),
                );
            } else {
                bail!("--n is required for loop_n workflow");
            }
        }
        WorkflowType::LoopUntilSentinel => {
            if let Some(p) = prompt {
                params.insert("prompt".to_string(), serde_json::Value::String(p.to_string()));
            } else {
                bail!("--prompt is required for loop_until_sentinel workflow");
            }
            if let Some(s) = sentinel {
                params.insert("sentinel".to_string(), serde_json::Value::String(s.to_string()));
            } else {
                bail!("--sentinel is required for loop_until_sentinel workflow");
            }
        }
        WorkflowType::Inbox => {
            // Inbox doesn't require prompt; uses agent_id from other params
        }
    }

    if let Some(ac) = agent_cli {
        params.insert(
            "agent_cli".to_string(),
            serde_json::Value::String(ac.to_string()),
        );
    }

    if let Some(bm) = branch_mode {
        params.insert(
            "branch_mode".to_string(),
            serde_json::Value::String(bm.to_string()),
        );
    }

    if let Some(m) = model {
        params.insert(
            "model".to_string(),
            serde_json::Value::String(m.to_string()),
        );
    }

    let req = CreateSessionRequest {
        repo_url: repo.to_string(),
        ref_name: ref_name.unwrap_or("main").to_string(),
        workflow: wf,
        params: serde_json::Value::Object(params),
        persona_id: persona_id.map(|s| s.to_string()),
        identity_id: None,
        retain_forever: false,
    };

    let resp = client.create_session(&req).await?;
    println!("Session created:");
    println!("  ID:     {}", resp.session_id);
    println!(
        "  Status: {}",
        serde_json::to_string(&resp.status).unwrap_or_default().trim_matches('"')
    );
    if let Some(url) = &resp.web_url {
        println!("  URL:    {}", url);
    }
    Ok(())
}

pub async fn list(client: &ApiClient, status: Option<&str>) -> Result<()> {
    let resp = client.list_sessions(status).await?;

    if resp.items.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    // Print table header
    println!(
        "{:<38} {:<12} {:<12} {:<40} CREATED",
        "SESSION ID", "STATUS", "WORKFLOW", "REPO"
    );
    println!("{}", "-".repeat(120));

    for item in &resp.items {
        let repo_display = if item.repo_url.len() > 38 {
            format!("...{}", &item.repo_url[item.repo_url.len() - 35..])
        } else {
            item.repo_url.clone()
        };
        println!(
            "{:<38} {:<12} {:<12} {:<40} {}",
            item.session_id,
            item.status,
            item.workflow,
            repo_display,
            item.created_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    if resp.next_cursor.is_some() {
        println!("\n(more sessions available; pagination not shown in v1 CLI)");
    }

    Ok(())
}

pub async fn show(client: &ApiClient, id: &str) -> Result<()> {
    let detail = client.get_session(id).await?;

    println!("Session: {}", detail.session_id);
    println!("  Status:    {}", detail.status);
    println!("  Workflow:  {}", detail.workflow);
    println!("  Repo:      {}", detail.repo_url);
    println!("  Ref:       {}", detail.ref_name);
    println!("  Created:   {}", detail.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("  Updated:   {}", detail.updated_at.format("%Y-%m-%d %H:%M:%S UTC"));

    if detail.params != serde_json::Value::Object(serde_json::Map::new()) {
        println!("  Params:    {}", serde_json::to_string_pretty(&detail.params)?);
    }

    if detail.jobs.is_empty() {
        println!("\n  No jobs.");
    } else {
        println!("\n  Jobs:");
        println!(
            "  {:<38} {:<12} {:<22} ERROR/PR",
            "JOB ID", "STATUS", "CREATED"
        );
        println!("  {}", "-".repeat(100));
        for job in &detail.jobs {
            let extra = if let Some(err) = &job.error_message {
                format!("Error: {}", err)
            } else if let Some(pr) = &job.pull_request_url {
                format!("PR: {}", pr)
            } else {
                String::new()
            };
            println!(
                "  {:<38} {:<12} {:<22} {}",
                job.job_id,
                job.status,
                job.created_at.format("%Y-%m-%d %H:%M:%S"),
                extra
            );
        }
    }

    Ok(())
}

pub async fn delete(client: &ApiClient, id: &str) -> Result<()> {
    client.delete_session(id).await?;
    println!("deleted");
    Ok(())
}
