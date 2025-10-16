use clap::{Arg, Command};
use log::{info};
use std::path::PathBuf;

use babymode::{Config, ConfigBuilder, ConfigFile, Result, WhisperModel};
use babymode::{dependencies, video, audio, whisper, plugins};
use babymode::{StrategyRegistry, ProgressOperation};

fn build_cli() -> Command {
    Command::new("babymode")
        .about("A multimedia application that automatically censors swearing in video files")
        .version("0.1.0")
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .help("Input video file to process")
                .required(false) // Will be validated in parse_config
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output video file (optional, defaults to input_censored.ext)")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("model")
                .short('m')
                .long("model")
                .value_name("MODEL")
                .help("Whisper model to use for transcription")
                .default_value("base")
                .value_parser(["tiny", "base", "small", "medium", "large"]),
        )
        .arg(
            Arg::new("volume")
                .short('v')
                .long("volume")
                .value_name("FLOAT")
                .help("Volume level during censoring (0.0-1.0)")
                .default_value("0.1")
                .value_parser(clap::value_parser!(f32)),
        )
        .arg(
            Arg::new("fade")
                .short('f')
                .long("fade")
                .value_name("SECONDS")
                .help("Fade duration in seconds")
                .default_value("0.2")
                .value_parser(clap::value_parser!(f32)),
        )
        .arg(
            Arg::new("words")
                .short('w')
                .long("words")
                .value_name("WORD,WORD,...")
                .help("Custom comma-separated list of words to censor")
                .value_delimiter(','),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .help("Enable verbose logging")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file (YAML/JSON)")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("profile")
                .short('p')
                .long("profile")
                .value_name("NAME")
                .help("Configuration profile to use (from config file)"),
        )
        .arg(
            Arg::new("strategy")
                .short('s')
                .long("strategy")
                .value_name("STRATEGY")
                .help("Censoring strategy to use")
                .default_value("silence")
                .value_parser(["silence", "volume_reduction", "beep", "reverse"]),
        )
        .arg(
            Arg::new("no-progress")
                .long("no-progress")
                .help("Disable progress indicators")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("list-profiles")
                .long("list-profiles")
                .help("List available configuration profiles")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("list-strategies")
                .long("list-strategies")
                .help("List available censoring strategies")
                .action(clap::ArgAction::SetTrue),
        )
}

async fn parse_config(matches: &clap::ArgMatches) -> Result<Config> {
    // Handle special listing commands first
    if matches.get_flag("list-strategies") {
        let registry = StrategyRegistry::new();
        println!("Available censoring strategies:");
        for (name, description) in registry.list_strategies() {
            println!("  {}: {}", name, description);
        }
        std::process::exit(0);
    }

    if matches.get_flag("list-profiles") {
        // Try to load config file to show profiles
        let config_file = if let Some(config_path) = matches.get_one::<PathBuf>("config") {
            ConfigFile::load(config_path).await.ok()
        } else {
            ConfigFile::load_from_default_locations().await
        }.unwrap_or_default();
        
        println!("Available configuration profiles:");
        for profile_name in config_file.list_profiles() {
            let profiles = config_file.profiles.as_ref().unwrap();
            let profile = profiles.get(&profile_name).unwrap();
            println!("  {}: {}", profile_name, 
                profile.description.as_deref().unwrap_or("No description"));
        }
        std::process::exit(0);
    }

    // Input file is only required for non-listing commands
    let input_file = if matches.get_flag("list-strategies") || matches.get_flag("list-profiles") {
        // For listing commands, we don't need an input file
        PathBuf::from("dummy") // Will never be used
    } else {
        matches
            .get_one::<PathBuf>("input")
            .ok_or_else(|| babymode::error::config_error("input", "Input file is required"))?
            .clone()
    };

    let mut builder = ConfigBuilder::new().input_file(input_file);
    
    // Load config file if specified or from default locations
    let config_file = if let Some(config_path) = matches.get_one::<PathBuf>("config") {
        Some(ConfigFile::load(config_path).await?)
    } else {
        ConfigFile::load_from_default_locations().await
    };
    
    // Apply config file settings
    if let Some(ref cf) = config_file {
        if let Some(profile_name) = matches.get_one::<String>("profile") {
            // Apply specific profile
            builder = cf.apply_profile_to_builder(profile_name, builder)?;
        } else {
            // Apply base config file settings
            builder = cf.apply_to_builder(builder)?;
        }
    }

    if let Some(output) = matches.get_one::<PathBuf>("output") {
        builder = builder.output_file(output.clone());
    }

    if let Some(model_str) = matches.get_one::<String>("model") {
        let model: WhisperModel = model_str.parse()?;
        builder = builder.whisper_model(model);
    }

    if let Some(&volume) = matches.get_one::<f32>("volume") {
        builder = builder.censor_volume(volume)?;
    }

    if let Some(&fade) = matches.get_one::<f32>("fade") {
        builder = builder.fade_duration(fade)?;
    }

    if let Some(words) = matches.get_many::<String>("words") {
        let word_list: Vec<String> = words.cloned().collect();
        builder = builder.swear_words(word_list)?;
    }

    builder.build()
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = build_cli();
    let matches = app.get_matches();

    // Initialize logging
    if matches.get_flag("verbose") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    let config = parse_config(&matches).await?;
    let show_progress = !matches.get_flag("no-progress");
    let strategy_name = matches.get_one::<String>("strategy").unwrap();
    
    let progress = ProgressOperation::new(show_progress);
    
    info!("Starting babymode with config: {:?}", config);
    
    // Validate system dependencies before processing
    progress.with_spinner("Validating system dependencies", |_pb| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                dependencies::validate_dependencies().await
            })
        })
    }).await?;

    // Validate input file is a video file
    progress.with_spinner("Validating input video file", |_pb| {
        video::validate_video_file(&config.input_file)
    }).await?;

    // Extract audio from video
    let temp_audio = progress.with_spinner("Extracting audio from video", |_pb| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                audio::extract_audio(&config.input_file).await
            })
        })
    }).await?;

    // Detect swear words using faster-whisper
    let detections = progress.with_spinner("Analyzing audio for swear words", |_pb| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                whisper::detect_swear_words(temp_audio.path(), &config).await
            })
        })
    }).await?;

    info!("Found {} swear word segments", detections.len());

    if detections.is_empty() {
        progress.with_spinner("No swear words detected, creating clean copy", |_pb| {
            std::fs::copy(&config.input_file, config.output_file.as_ref().unwrap())
                .map_err(|e| babymode::error::fs_error(e, config.input_file.clone()))
        }).await?;
        info!("Clean copy created at: {:?}", config.output_file.unwrap());
        return Ok(());
    }

    // Apply censoring using selected strategy
    let registry = StrategyRegistry::new();
    let censoring_config = plugins::CensoringConfig {
        volume: config.censor_volume,
        fade_duration: config.fade_duration,
        ..Default::default()
    };
    
    let temp_censored_audio = progress.with_spinner(
        &format!("Applying {} censoring strategy", strategy_name), 
        |_pb| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let temp_file = tempfile::NamedTempFile::new()
                        .map_err(|e| babymode::BabymodeError::Processing {
                            message: format!("Failed to create temp file: {}", e)
                        })?;
                    let temp_path = temp_file.path().to_path_buf();
                    let temp_output = babymode::TempFile::new(temp_path);
                    
                    let segments: Vec<_> = detections.iter()
                        .map(|d| d.to_audio_segment())
                        .collect();
                    
                    registry.apply_strategy(
                        strategy_name,
                        temp_audio.path(),
                        temp_output.path(),
                        &segments,
                        &censoring_config,
                    ).await?;
                    
                    Ok::<_, babymode::BabymodeError>(temp_output)
                })
            })
        }
    ).await?;

    // Combine censored audio with original video
    progress.with_spinner("Creating final censored video", |_pb| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                video::combine_video_audio(
                    &config.input_file,
                    temp_censored_audio.path(),
                    config.output_file.as_ref().unwrap()
                ).await
            })
        })
    }).await?;
    
    info!("✓ Successfully created censored video: {:?}", config.output_file.unwrap());
    info!("Strategy used: {}", strategy_name);
    info!("Censored {} segments", detections.len());
    
    // Temporary files will be automatically cleaned up when temp_audio and temp_censored_audio go out of scope

    Ok(())
}