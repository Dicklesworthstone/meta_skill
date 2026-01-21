//! ms embed - Test embedding backends
//!
//! Utility command for testing and debugging embedding backends.

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::Result;
use crate::search::embeddings::build_embedder;

#[derive(Args, Debug)]
pub struct EmbedArgs {
    /// Text to embed
    pub text: String,

    /// Override embedding backend (hash, api, local)
    #[arg(long, short)]
    pub backend: Option<String>,

    /// Show full embedding vector (default: summary only)
    #[arg(long)]
    pub full: bool,

    /// Compare with another text (show similarity)
    #[arg(long, short)]
    pub compare: Option<String>,
}

pub fn run(ctx: &AppContext, args: &EmbedArgs) -> Result<()> {
    // Override backend if specified
    let mut config = ctx.config.search.clone();
    if let Some(ref backend) = args.backend {
        config.embedding_backend = backend.clone();
    }

    if ctx.output_format != OutputFormat::Human {
        run_robot(ctx, args, &config)
    } else {
        run_human(ctx, args, &config)
    }
}

fn run_human(
    _ctx: &AppContext,
    args: &EmbedArgs,
    config: &crate::config::SearchConfig,
) -> Result<()> {
    let embedder = build_embedder(config)?;

    println!("{}", "Embedding Configuration".bold());
    println!("  Backend: {}", embedder.name().cyan());
    println!("  Dimensions: {}", embedder.dims().to_string().cyan());
    println!();

    println!("{}", "Input Text".bold());
    println!("  \"{}\"", args.text.green());
    println!();

    let embedding = embedder.embed(&args.text);

    println!("{}", "Embedding Result".bold());
    println!("  Length: {} floats", embedding.len());

    // Calculate basic statistics
    let (min, max, sum) = embedding
        .iter()
        .fold((f32::MAX, f32::MIN, 0.0f32), |acc, &x| {
            (acc.0.min(x), acc.1.max(x), acc.2 + x)
        });
    let mean = sum / embedding.len() as f32;
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

    println!("  Min: {min:.6}");
    println!("  Max: {max:.6}");
    println!("  Mean: {mean:.6}");
    println!("  L2 Norm: {norm:.6}");

    // Non-zero count (for sparse embeddings)
    let non_zero = embedding.iter().filter(|&&x| x.abs() > 1e-10).count();
    println!(
        "  Non-zero: {} ({:.1}%)",
        non_zero,
        100.0 * non_zero as f32 / embedding.len() as f32
    );

    if args.full {
        println!();
        println!("{}", "Full Vector".bold());
        for (i, v) in embedding.iter().enumerate() {
            if i > 0 && i % 8 == 0 {
                println!();
            }
            print!("{v:8.5} ");
        }
        println!();
    }

    // Similarity comparison if requested
    if let Some(ref compare_text) = args.compare {
        println!();
        println!("{}", "Similarity Comparison".bold());
        println!("  Text A: \"{}\"", args.text.green());
        println!("  Text B: \"{}\"", compare_text.green());

        let embedding_b = embedder.embed(compare_text);

        // Calculate cosine similarity
        let dot: f32 = embedding
            .iter()
            .zip(embedding_b.iter())
            .map(|(a, b)| a * b)
            .sum();
        let norm_a: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = embedding_b.iter().map(|x| x * x).sum::<f32>().sqrt();
        let similarity = if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        };

        let sim_color = if similarity > 0.8 {
            "green"
        } else if similarity > 0.5 {
            "yellow"
        } else {
            "red"
        };

        println!(
            "  Cosine Similarity: {}",
            format!("{similarity:.4}").color(sim_color)
        );
    }

    Ok(())
}

fn run_robot(
    _ctx: &AppContext,
    args: &EmbedArgs,
    config: &crate::config::SearchConfig,
) -> Result<()> {
    let embedder = build_embedder(config)?;
    let embedding = embedder.embed(&args.text);

    let mut output = serde_json::json!({
        "status": "ok",
        "backend": embedder.name(),
        "dimensions": embedder.dims(),
        "input": args.text,
        "embedding_length": embedding.len(),
    });

    if args.full {
        output["embedding"] = serde_json::json!(embedding);
    }

    // Calculate statistics
    let (min, max, sum) = embedding
        .iter()
        .fold((f32::MAX, f32::MIN, 0.0f32), |acc, &x| {
            (acc.0.min(x), acc.1.max(x), acc.2 + x)
        });
    let mean = sum / embedding.len() as f32;
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    let non_zero = embedding.iter().filter(|&&x| x.abs() > 1e-10).count();

    output["stats"] = serde_json::json!({
        "min": min,
        "max": max,
        "mean": mean,
        "l2_norm": norm,
        "non_zero_count": non_zero,
        "non_zero_pct": 100.0 * non_zero as f32 / embedding.len() as f32,
    });

    if let Some(ref compare_text) = args.compare {
        let embedding_b = embedder.embed(compare_text);
        let dot: f32 = embedding
            .iter()
            .zip(embedding_b.iter())
            .map(|(a, b)| a * b)
            .sum();
        let norm_a: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = embedding_b.iter().map(|x| x * x).sum::<f32>().sqrt();
        let similarity = if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        };

        output["comparison"] = serde_json::json!({
            "text_b": compare_text,
            "cosine_similarity": similarity,
        });
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_args_parses() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: EmbedArgs,
        }

        let cli = TestCli::parse_from(["test", "hello world"]);
        assert_eq!(cli.args.text, "hello world");
        assert!(cli.args.backend.is_none());
        assert!(!cli.args.full);
    }
}
