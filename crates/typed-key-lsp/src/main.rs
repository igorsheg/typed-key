use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use std::fs;
use std::path::PathBuf;
use tower_lsp::{LspService, Server};
use typed_key::generate::TypeScriptGenerator;
use typed_key::lsp::backend::TypedKeyLsp;
use typed_key::{Lexer as TypedKeyLexer, Parser as TypedKeyParser};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Tokenize {
        filename: PathBuf,
    },
    Parse {
        filename: PathBuf,
    },
    GenerateTypes {
        input_dir: PathBuf,
        output_file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    match args.command {
        Some(Commands::Tokenize { filename }) => tokenize(filename),
        Some(Commands::Parse { filename }) => parse_file(filename),
        Some(Commands::GenerateTypes {
            input_dir,
            output_file,
        }) => generate_types(input_dir, output_file),
        None => start_lsp().await,
    }
}

fn tokenize(filename: PathBuf) -> Result<()> {
    let file_contents = fs::read_to_string(&filename)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading '{}' failed", filename.display()))?;
    let lexer = TypedKeyLexer::new(&file_contents);
    for token in lexer {
        println!("{:?}", token);
    }
    Ok(())
}

fn parse_file(filename: PathBuf) -> Result<()> {
    let file_contents = fs::read_to_string(&filename)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading '{}' failed", filename.display()))?;
    let parser = TypedKeyParser::new(&file_contents);
    let parsed = parser.parse().unwrap();
    println!("Trans {:?}", parsed);
    Ok(())
}

fn generate_types(input_dir: PathBuf, output_file: PathBuf) -> Result<()> {
    let mut generator = TypeScriptGenerator::new();
    generator
        .process_directory(input_dir.to_str().unwrap())
        .into_diagnostic()
        .wrap_err_with(|| format!("processing directory '{}' failed", input_dir.display()))?;
    generator
        .generate_typescript_definitions(output_file.to_str().unwrap())
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "generating TypeScript definitions in '{}' failed",
                output_file.display()
            )
        })?;
    println!(
        "TypeScript definitions generated successfully in '{}'",
        output_file.display()
    );
    Ok(())
}

async fn start_lsp() -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(TypedKeyLsp::new);

    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}
