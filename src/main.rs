use std::{io::Read, path::PathBuf};

use console::{style, StyledObject, Term};
use indicatif::ProgressIterator;
use structopt::StructOpt;
use timer::{Status, Timer};

mod parser;
mod timer;

#[derive(Debug, StructOpt)]
#[structopt(name = "graphql-field-timer")]
struct Opt {
    #[structopt(short, long, parse(from_os_str))]
    file: Option<PathBuf>,

    #[structopt(long)]
    header: Vec<String>,

    #[structopt(short, long)]
    url: String,

    #[structopt(short, long)]
    variables: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    // Parse the GraphQL queries into the individual field queries we're going
    // to send.
    let raw = String::from_utf8(opt.file.map(std::fs::read).unwrap_or_else(|| {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        Ok(buf)
    })?)?;
    let doc = graphql_parser::parse_query::<&str>(&raw)?;
    let queries = parser::parse_document(&doc);

    // Set up the timer.
    let mut timer = Timer::new(&opt.url, opt.header, opt.variables)?;

    // Actually send the GraphQL queries.
    for query in queries.into_iter().progress() {
        timer.send_query(&query).await?;
    }

    // Output our results.
    for result in timer.results().into_iter() {
        println!(
            "{} {} {}",
            render_status(result.status),
            style(format!(" {:.3}s ", result.duration.as_secs_f64())).dim(),
            result.query,
        );
        if result.status == Status::Failure {
            println!("{}", result.dump_response());
        }
    }

    Ok(())
}

fn render_status(status: Status) -> StyledObject<String> {
    match status {
        Status::Success => style(" OK  ".into()).black().on_green(),
        Status::Failure => style(" ERR ".into()).white().on_red(),
    }
    .bright()
    .bold()
}
