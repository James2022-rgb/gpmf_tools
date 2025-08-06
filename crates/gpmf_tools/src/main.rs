
mod logging;

use std::io::Write;
use std::fs::File;

use log::trace;
use clap::{Parser, Subcommand, Args};
use mp4::{Mp4Reader, FourCC};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, global = true, help = "Verbose logging.", default_value_t = false)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[cfg(all(feature = "gpx", feature = "mp4"))]
    #[command(name = "extract-gpx", about = "Extracts GPS data from a GoPro MP4 file and saves it as a GPX file.")]
    ExtractGpx(ExtractGpxArgs),
}

#[cfg(all(feature = "gpx", feature = "mp4"))]
#[derive(Args, Debug)]
struct ExtractGpxArgs {
    /// The input file to process.
    #[arg(short='i', long="input")]
    input_file_path: String,
    /// The output file to write to.
    #[arg(short='o', long="output")]
    output_file_path: Option<String>,
    /// Write output to stdout if true.
    /// If not specified, the program will write to stdout if `--stdout` is provided.
    #[arg(long="stdout", default_value_t = false)]
    stdout: bool,
}

fn main() -> Result<(), String>  {
    let cli = Cli::parse();

    logging::LoggingConfig::default()
        .level(if cli.verbose {
            log::LevelFilter::Trace
        } else {
            log::LevelFilter::Info
        })
        .apply();

    match cli.command {
        #[cfg(all(feature = "gpx", feature = "mp4"))]
        Commands::ExtractGpx(args) => {
            trace!("Extracting GPX from file: {}", args.input_file_path);

            let in_file = File::open(args.input_file_path)
                .map_err(|e| format!("Failed to open input file: {}", e))?;
            let in_file_size = in_file.metadata()
                .map_err(|e| format!("Failed to get input file size: {}", e))?
                .len();

            let mut mp4_reader = Mp4Reader::read_header(in_file, in_file_size)
                .map_err(|e| format!("Failed to read MP4 header: {}", e))?;

            let gpmf_track_id = mp4_reader
                .tracks()
                .iter()
                .find(|&(_, track)| {
                    track.trak.mdia.hdlr.handler_type == FourCC::from(0x6D657461 /* "meta" */)
                        && track.trak.mdia.hdlr.name.contains("GoPro MET")
                })
                .map(|(track_id, _)| *track_id);
            let gpmf_track_id = gpmf_track_id.ok_or_else(|| "No GPMF track found in the MP4 file".to_string())?;

            let gpmf_track = gpmf_util::GpmfTrack::from_mp4_reader(&mut mp4_reader, gpmf_track_id)
                .map_err(|e| format!("Failed to read GPMF track: {}", e))?;

            trace!("GPMF sample count: {}", gpmf_track.gpmf_sample_infos().len());

            let mut writer: Box<dyn Write> = if let Some(output_file_path) = args.output_file_path {
                trace!("Writing output to file: {}", output_file_path);
                Box::new(File::create(output_file_path).map_err(|e| format!("Failed to create output file: {}", e))?)
            } else if args.stdout {
                trace!("Writing output to stdout");
                Box::new(std::io::stdout())
            } else {
                return Err("No output file specified and stdout not enabled".to_string());
            };

            gpmf_track.write_gpx(&mut writer)
                .map_err(|e| format!("Failed to write GPX: {}", e))?;

            Ok(())
        }
    }
}
