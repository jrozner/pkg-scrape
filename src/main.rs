use clap::Parser;
use futures::{stream, StreamExt};
use log::error;
use log;
use reqwest;
use scraper::{Html, Selector};
use simple_logger;
use std::collections::HashSet;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    module: String,

    #[arg(short, default_value_t = 16)]
    tasks: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let args = Args::parse();
    let url = format!("https://pkg.go.dev/{}?tab=importedby", args.module);

    let resp = reqwest::get(url).await?;
    let doc = Html::parse_document(&resp.text().await?);
    let selector = Selector::parse(".ImportedBy-detailsIndent")?;
    let importers = doc
        .select(&selector)
        .filter_map(|m| {
            if m.text().count() > 1 {
                error!("{} elements found when expecting 1", m.text().count());
            }

            if let Some(path) = m.text().next() {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let results = stream::iter(importers)
        .map(lookup_module)
        .buffer_unordered(args.tasks)
        .filter_map(|res| async {
            match res {
                Ok(s) => Some(s),
                Err(e) => {
                    error!("unable to lookup module: {}", e);
                    None
                }
            }
        })
        .collect::<HashSet<String>>()
        .await;

    println!("{:#?}", results);
    Ok(())
}

async fn lookup_module(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("https://pkg.go.dev/{path}");
    let resp = reqwest::get(url).await?;
    let selector = Selector::parse(".go-Main-headerBreadcrumb li:nth-child(2) a")?;
    let doc = Html::parse_document(&resp.text().await?);
    let import = doc
        .select(&selector)
        .next()
        .ok_or("no element found")?
        .text()
        .next()
        .ok_or("no text")?
        .trim()
        .into();
    Ok(import)
}
