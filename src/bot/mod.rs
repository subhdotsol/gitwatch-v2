pub mod callbacks;
pub mod commands;
pub mod notify;

use teloxide::{
    prelude::*,
    types::{BotCommand, BotCommandScope},
};

use crate::state::AppState;

pub async fn run_bot(state: AppState) {
    let bot = state.bot.clone();

    // Set bot commands
    let commands = vec![
        BotCommand::new("start", "Connect your GitHub account"),
        BotCommand::new("watch", "Watch a repository"),
        BotCommand::new("watchlist", "View all watched repositories"),
        BotCommand::new("unwatch", "Stop watching a repository"),
        BotCommand::new("status", "Check your plan and usage"),
        BotCommand::new("disconnect", "Disconnect GitHub and remove all watches"),
        BotCommand::new("help", "Show help message"),
    ];

    if let Err(e) = bot
        .set_my_commands(commands)
        .scope(BotCommandScope::Default)
        .await
    {
        tracing::warn!("Failed to set bot commands: {e}");
    }

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<commands::Command>()
                .endpoint({
                    let state = state.clone();
                    move |bot, msg, cmd| {
                        let state = state.clone();
                        commands::handle_command(bot, msg, cmd, state)
                    }
                }),
        )
        .branch(Update::filter_callback_query().endpoint({
            let state = state.clone();
            move |bot, q| {
                let state = state.clone();
                callbacks::handle_callback(bot, q, state)
            }
        }));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}
