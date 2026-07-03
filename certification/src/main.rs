mod damage;
mod coverage;
mod crash;
mod media;
mod phases;
mod reporters;
mod validators;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::prelude::*;

#[derive(Parser, Debug)]
#[command(name = "certification_runner")]
#[command(about = "Playlist Merger Runtime Certification Framework", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "./media_assets")]
    media_path: PathBuf,

    #[arg(short, long)]
    binary_path: Option<PathBuf>,

    #[arg(short, long, default_value = "./reports")]
    output_path: PathBuf,

    #[arg(short, long, default_value = "./test_output")]
    temp_path: PathBuf,

    #[command(subcommand)]
    phase: Option<Phase>,

    #[arg(long)]
    skip_missing_media: bool,

    #[arg(short, long)]
    quiet: bool,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Debug, Clone)]
enum Phase {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    All,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let filter = if args.verbose {
        tracing_subscriber::filter::LevelFilter::DEBUG
    } else if args.quiet {
        tracing_subscriber::filter::LevelFilter::WARN
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter))
        .init();

    tracing::info!("Playlist Merger Certification Framework");
    tracing::info!("Media path: {:?}", args.media_path);
    tracing::info!("Output path: {:?}", args.output_path);
    tracing::info!("Temp path: {:?}", args.temp_path);

    std::fs::create_dir_all(&args.output_path)?;
    std::fs::create_dir_all(&args.temp_path)?;

    let media_assets = media::MediaAssets::discover(&args.media_path)?;

    if !args.quiet {
        media_assets.print_summary();
    }

    if media_assets.missing_count() > 0 && args.skip_missing_media {
        tracing::warn!(
            "Missing {} media assets, skipping tests that require them",
            media_assets.missing_count()
        );
    }

    let runner = certification::Runner::new(
        args.binary_path,
        args.media_path.clone(),
        args.output_path.clone(),
        args.temp_path.clone(),
        media_assets,
        args.quiet,
    );

    let results = match args.phase.as_ref().cloned().unwrap_or(Phase::All) {
        Phase::A => runner.run_phase_a()?,
        Phase::B => runner.run_phase_b()?,
        Phase::C => runner.run_phase_c()?,
        Phase::D => runner.run_phase_d()?,
        Phase::E => runner.run_phase_e()?,
        Phase::F => runner.run_phase_f()?,
        Phase::G => runner.run_phase_g()?,
        Phase::H => runner.run_phase_h()?,
        Phase::I => runner.run_phase_i()?,
        Phase::J => runner.run_phase_j()?,
        Phase::K => runner.run_phase_k()?,
        Phase::L => runner.run_phase_l()?,
        Phase::All => {
            let mut all_results = Vec::new();
            all_results.extend(runner.run_phase_a()?);
            all_results.extend(runner.run_phase_b()?);
            all_results.extend(runner.run_phase_c()?);
            all_results.extend(runner.run_phase_d()?);
            all_results.extend(runner.run_phase_e()?);
            all_results.extend(runner.run_phase_f()?);
            all_results.extend(runner.run_phase_g()?);
            all_results.extend(runner.run_phase_h()?);
            all_results.extend(runner.run_phase_j()?);
            all_results.extend(runner.run_phase_k()?);
            all_results.extend(runner.run_phase_l()?);
            all_results
        }
    };

    let report = reporters::FinalReport::generate(&results);
    let report_path = args.output_path.join("certification_report.yaml");
    report.save(&report_path)?;
    let summary_path = args.output_path.join("certification_summary.txt");
    report.save_summary(&summary_path)?;

    println!("\n{}", report);

    if args.verbose {
        println!("\nReport saved to: {:?}", report_path);
        println!("Summary saved to: {:?}", summary_path);
    }

    Ok(())
}

pub mod certification {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Arc;

    pub struct Runner {
        binary_path: Option<PathBuf>,
        media_path: PathBuf,
        output_path: PathBuf,
        temp_path: PathBuf,
        media_assets: media::MediaAssets,
        quiet: bool,
        results: Arc<Mutex<Vec<reporters::TestResult>>>,
    }

    impl Runner {
        pub fn new(
            binary_path: Option<PathBuf>,
            media_path: PathBuf,
            output_path: PathBuf,
            temp_path: PathBuf,
            media_assets: media::MediaAssets,
            quiet: bool,
        ) -> Self {
            Self {
                binary_path,
                media_path,
                output_path,
                temp_path,
                media_assets,
                quiet,
                results: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn record(&self, result: reporters::TestResult) {
            if !self.quiet {
                let status = match result.status {
                    reporters::TestStatus::Pass => "PASS",
                    reporters::TestStatus::Fail => "FAIL",
                    reporters::TestStatus::Warn => "WARN",
                    reporters::TestStatus::Skip => "SKIP",
                };
                println!("  {} - {}", status, result.name);
            }
            self.results.lock().push(result);
        }

        pub fn run_phase_a(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE A: CORE MERGE CERTIFICATION ===");
            let results = phases::phase_a::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_b(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE B: REPEAT CERTIFICATION ===");
            let results = phases::phase_b::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_c(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE C: SPLIT CERTIFICATION ===");
            let results = phases::phase_c::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_d(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE D: CARDS CERTIFICATION ===");
            let results = phases::phase_d::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_e(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE E: RECOVERY CERTIFICATION ===");
            let results = phases::phase_e::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_f(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE F: AUDIO CERTIFICATION ===");
            let results = phases::phase_f::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_g(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE G: SUBTITLE CERTIFICATION ===");
            let results = phases::phase_g::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_h(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE H: LARGE PLAYLIST STRESS TESTING ===");
            let results = phases::phase_h::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_i(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE I: FINAL REPORT GENERATION ===");
            let results = self.results.lock().clone();
            let report = reporters::FinalReport::generate(&results);
            println!("\n{}", report);
            Ok(results)
        }

        pub fn run_phase_j(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE J: PRODUCTION STABILITY CERTIFICATION ===");
            println!("NOTE: Phase J runs runtime stability tests via the Tauri backend.");
            println!("To run full stability certification, start the SmartMKV application.");
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_k(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE K: REPAIR COVERAGE CERTIFICATION ===");
            println!("Verifies every production repair path has empirical test coverage.");
            let results = phases::phase_k::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }

        pub fn run_phase_l(&self) -> Result<Vec<reporters::TestResult>> {
            println!("\n=== PHASE L: END-TO-END PIPELINE VERIFICATION ===");
            println!("Verifies complete production pipeline handles damaged media correctly.");
            let results = phases::phase_l::run(&self.binary_path, &self.media_path, &self.temp_path, &self.media_assets)?;
            for r in results { self.record(r); }
            Ok(self.results.lock().clone())
        }
    }
}