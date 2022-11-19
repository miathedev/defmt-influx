use core::time;
use std::time::Duration;
use defmt_decoder::Table;
use defmt_decoder::{DecodeError, Frame, Locations, StreamDecoder};
use futures::prelude::*;
use influxdb2::api::buckets::ListBucketsRequest;
use influxdb2::api::organization::ListOrganizationRequest;
use influxdb2::models::{DataPoint, PostBucketRequest};
use influxdb2::Client;
use tokio::time::sleep;
use std::env;
use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;

/*
OPTIONS:
        --influx_bucket <influx bucket>
        --influx_host <influx host url>     [default: 127.0.0.1]
        --influx_org <influx org>           [default: test]
        --influx_token <influx token>
        --rtt_host <rtt host>               [default: 127.0.0.1]
        --rtt_port <rtt port>
*/
/// Serial errors
#[derive(Debug, thiserror::Error)]
pub enum SerialError {
    #[error("Invalid parity requested \"{0}\"")]
    InvalidParityString(String),
    #[error("Invalid stop bits requested \"{0}\"")]
    InvalidStopBitsString(String),
    #[error("Defmt data not found")]
    DefmtDataNotFound,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "basic")]
struct Opts {
    /// Path to the elf file with defmt metadata
    #[structopt(name = "elf", required(true), long = "elf")]
    elf: PathBuf,

    #[structopt(name = "rtt port", required(true), long = "rtt_port")]
    rtt_port: String,

    #[structopt(name = "rtt host", required(true), long = "rtt_host")]
    rtt_host: String,

    #[structopt(name = "influx host url", required(true), long = "influx_host")]
    influx_host: String,

    #[structopt(name = "influx org", required(true), long = "influx_org")]
    influx_org: String,

    #[structopt(name = "influx token", required(true), long = "influx_token")]
    influx_token: String,

    #[structopt(name = "influx bucket", required(true), long = "influx_bucket")]
    influx_bucket: String,

    #[structopt(name = "influx meassurement to log to", required(true), long = "influx_meassurement")]
    influx_meassurement: String,

    /// Shows defmt parsing errors. By default these are ignored.
    #[structopt(long, short = "d")]
    display_parsing_errors: bool,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::from_args();

    let verbose = false;
    defmt_decoder::log::init_logger(verbose, |_| true);

    let current_dir = &env::current_dir()?;

    let elf_data = std::fs::read(&opts.elf)?;
    let (table, locations) = extract_defmt_info(&elf_data)?;
    let table = table.unwrap();

    let mut decoder_and_encoding = (table.new_stream_decoder(), table.encoding());

    let connection_string = format!("{}:{}", opts.rtt_host, opts.rtt_port);

    //InfluxDB
    let client = Client::new(opts.influx_host.clone(), opts.influx_org, opts.influx_token);

    let mut read_buf = [0; 1024];
    log::info!("Start loop");
    loop {
        match TcpStream::connect(connection_string.clone()) {
            Ok(mut tcpclient) => loop {
                let num_bytes_read = match tcpclient.read(&mut read_buf) {
                    Ok(count) => Ok(count),
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Ok(0),
                    Err(error) => {
                        log::warn!("GNA");
                        Err(error)
                    }
                }?;

                if num_bytes_read != 0 {
                    let (stream_decoder, encoding) = &mut decoder_and_encoding;
                    stream_decoder.received(&read_buf[..num_bytes_read]);

                    match decode_and_print_defmt_logs_influx(
                        &mut **stream_decoder,
                        locations.as_ref(),
                        current_dir,
                        encoding.can_recover(),
                        client.to_owned(),
                        &opts.influx_bucket,
                        &opts.rtt_host.as_str(),
                        &opts.rtt_port.as_str(),
                        &opts.influx_meassurement.as_str()
                    )
                    .await
                    {
                        Ok(_) => {}
                        Err(error) => {
                            if opts.display_parsing_errors {
                                log::error!("Error parsing uart data: {}", error);
                            }
                        }
                    }
                } else {
                    log::warn!("Connection lost to rtt server");
                    break;
                }
            },
            Err(_) => {
                log::warn!("Failed to connect, retrying...");
                sleep(Duration::from_millis(1000)).await;
            },
        }
    }
    Ok(())
}

fn extract_defmt_info(elf_bytes: &[u8]) -> anyhow::Result<(Option<Table>, Option<Locations>)> {
    let defmt_table = match env::var("PROBE_RUN_IGNORE_VERSION").as_deref() {
        Ok("true") | Ok("1") => defmt_decoder::Table::parse_ignore_version(elf_bytes)?,
        _ => defmt_decoder::Table::parse(elf_bytes)?,
    };

    let mut defmt_locations = None;

    if let Some(table) = defmt_table.as_ref() {
        let locations = table.get_locations(elf_bytes)?;

        if !table.is_empty() && locations.is_empty() {
            log::warn!("insufficient DWARF info; compile your program with `debug = 2` to enable location info");
        } else if table
            .indices()
            .all(|idx| locations.contains_key(&(idx as u64)))
        {
            defmt_locations = Some(locations);
        } else {
            log::warn!("(BUG) location info is incomplete; it will be omitted from the output");
        }
    }

    Ok((defmt_table, defmt_locations))
}

async fn decode_and_print_defmt_logs_influx(
    stream_decoder: &mut dyn StreamDecoder,
    locations: Option<&Locations>,
    current_dir: &Path,
    encoding_can_recover: bool,
    client: Client,
    bucket: &str,
    rtt_host: &str,
    rtt_port: &str,
    influx_meassurement: &str
) -> anyhow::Result<()> {
    loop {
        match stream_decoder.decode() {
            Ok(frame) => {
                forward_to_logger(&frame, locations, current_dir);
                //forward_to_logger(&frame, locations, current_dir)
                let (file, line, mod_path) = location_info(&frame, locations, current_dir);

                let level_string = frame.level().unwrap().as_str();
                let point = DataPoint::builder(influx_meassurement)
                    .tag("rtt_host", rtt_host)
                    .tag("rtt_port", rtt_port)
                    .field("file", file.unwrap().as_str())
                    .field("mod_path", mod_path.unwrap().as_str())
                    .field("line", line.unwrap() as i64)
                    .field("msg", frame.display_message().to_string().as_str())
                    .tag("level", level_string)
                    .field(
                        "timestamp",
                        frame.display_timestamp().unwrap().to_string().as_str(),
                    )
                    .build()?;

                let points = vec![point];
                match client.write(bucket, stream::iter(points)).await {
                    Ok(_) => {
                        //log::info!("Add log data point");
                    }
                    Err(err) => {
                        log::error!("Failed to write data point to influxdb: {}", err);
                    }
                }
            }
            Err(DecodeError::UnexpectedEof) => break,
            Err(DecodeError::Malformed) => match encoding_can_recover {
                // if recovery is impossible, abort
                false => return Err(DecodeError::Malformed.into()),
                // if recovery is possible, skip the current frame and continue with new data
                true => continue,
            },
        }
    }

    Ok(())
}

fn forward_to_logger(frame: &Frame, locations: Option<&Locations>, current_dir: &Path) {
    let (file, line, mod_path) = location_info(frame, locations, current_dir);
    defmt_decoder::log::log_defmt(frame, file.as_deref(), line, mod_path.as_deref());
}

fn location_info(
    frame: &Frame,
    locations: Option<&Locations>,
    current_dir: &Path,
) -> (Option<String>, Option<u32>, Option<String>) {
    locations
        .map(|locations| &locations[&frame.index()])
        .map(|location| {
            let path = if let Ok(relpath) = location.file.strip_prefix(&current_dir) {
                relpath.display().to_string()
            } else {
                location.file.display().to_string()
            };
            (
                Some(path),
                Some(location.line as u32),
                Some(location.module.clone()),
            )
        })
        .unwrap_or((None, None, None))
}
