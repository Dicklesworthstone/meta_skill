//! ms inbox - Agent Mail inbox

use clap::Args;

use crate::agent_mail::{AgentMailClient, InboxMessage};
use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_human, emit_robot, robot_ok};
use crate::error::Result;

#[derive(Args, Debug)]
pub struct InboxArgs {
    /// Maximum number of messages to fetch
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Include full message bodies
    #[arg(long)]
    pub include_bodies: bool,

    /// Acknowledge a specific message by id
    #[arg(long)]
    pub ack: Option<i64>,

    /// Acknowledge all fetched messages
    #[arg(long)]
    pub ack_all: bool,
}

pub fn run(ctx: &AppContext, args: &InboxArgs) -> Result<()> {
    let mut client = AgentMailClient::from_config(&ctx.config.agent_mail)?;

    if let Some(id) = args.ack {
        client.acknowledge(id)?;
        if ctx.robot_mode {
            emit_robot(&robot_ok(serde_json::json!({
                "acknowledged": [id],
            })))?;
        } else {
            println!("Acknowledged message {id}.");
        }
        return Ok(());
    }

    let messages = client.fetch_inbox(args.limit, args.include_bodies)?;

    let mut acked = Vec::new();
    if args.ack_all {
        for message in &messages {
            client.acknowledge(message.id)?;
            acked.push(message.id);
        }
    }

    if ctx.robot_mode {
        emit_robot(&robot_ok(serde_json::json!({
            "agent": client.agent_name(),
            "project": client.project_key(),
            "messages": messages,
            "acknowledged": acked,
        })))
    } else {
        inbox_human(&client, &messages, args, &acked)
    }
}

fn inbox_human(
    client: &AgentMailClient,
    messages: &[InboxMessage],
    args: &InboxArgs,
    acked: &[i64],
) -> Result<()> {
    let mut layout = HumanLayout::new();
    layout
        .title("Agent Mail Inbox")
        .kv("Agent", client.agent_name())
        .kv("Project", client.project_key())
        .kv("Fetched", &messages.len().to_string());

    if !acked.is_empty() {
        layout.kv("Acknowledged", &acked.len().to_string());
    }

    if messages.is_empty() {
        layout.blank().push_line("No messages.");
        emit_human(layout);
        return Ok(());
    }

    for message in messages {
        layout
            .blank()
            .section(&format!("#{} {}", message.id, message.subject))
            .kv("From", &message.from)
            .kv("When", &message.created_ts)
            .kv("Importance", &message.importance)
            .kv("Ack Required", if message.ack_required { "yes" } else { "no" })
            .kv("Kind", &message.kind);

        if args.include_bodies {
            if let Some(body) = &message.body_md {
                layout.blank().push_line(body.clone());
            }
        }
    }

    emit_human(layout);
    Ok(())
}
