use clap::{Parser, ValueEnum};
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

    #[arg(short, default_value_t = 6)]
    tasks: usize,

    #[arg(short, long, value_enum, default_value_t = Output::Default)]
    output: Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Output {
    Default,
    Github,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let args = Args::parse();
    let url = format!("https://pkg.go.dev/{}?tab=importedby", args.module);

    let resp = reqwest::get(url).await?;
    resp.error_for_status_ref()?;
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

    if args.output == Output::Github {
        let filtered = results.iter().filter_map(|val| {
            if !val.starts_with("github.com") {
                return None
            }

            Some(val[11..].to_owned())
        }).collect::<Vec<String>>();
        println!("{:#?}", filtered);
    } else {
        println!("{:#?}", results);
    }

    Ok(())
}

async fn lookup_module(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("https://pkg.go.dev/{path}");
    let resp = reqwest::get(url).await?;
    resp.error_for_status_ref()?;
    let selector = Selector::parse(".go-Main-headerBreadcrumb li:nth-child(2) a")?;
    let body = resp.text().await?;
    let doc = Html::parse_document(&body);
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
