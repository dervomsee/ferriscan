//! Command-line composition root for the iX500 implementation.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::adapters::ix500::discovery::ScannerInfo;
use crate::adapters::ix500::identity;
use crate::adapters::ix500::{Ix500Locator, Ix500Scanner};
use crate::adapters::output::FileScanOutput;
use crate::adapters::scansnap_wifi::identity::{format_hex, local_identity_for, parse_mac};
use crate::core::{ColorMode, ImageInfo, MultifeedDetection, PageSize, Result, ScanSettings};
use crate::ports::{ScanOutput, ScannerLocator, ScannerPort};

const PASSWORD_ENV: &str = "FERRISCAN_IX500_PASSWORD";

#[derive(Debug, Parser)]
#[command(name = "ferriscan", version, about = "Independent ScanSnap scanning")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// ScanSnap iX500 Wi-Fi protocol commands.
    Ix500(Ix500Args),
}

#[derive(Debug, Args)]
struct Ix500Args {
    /// Override the MAC address of the local interface used to reach the scanner.
    #[arg(long, global = true, value_parser = parse_mac_arg)]
    mac: Option<[u8; 6]>,
    /// Identity file storing the paired host ID, MAC, and scanner password.
    #[arg(long, global = true)]
    identity: Option<PathBuf>,
    /// Never create or update the identity file.
    #[arg(long, global = true)]
    no_save: bool,
    /// Timeout per discovery attempt in seconds.
    #[arg(long, global = true, default_value_t = 1)]
    timeout: u64,
    #[command(subcommand)]
    command: Ix500Command,
}

#[derive(Debug, Subcommand)]
enum Ix500Command {
    /// Discover iX500 scanners over UDP.
    Discover {
        /// Probe only this address instead of searching the local network.
        #[arg(long, alias = "ip")]
        target: Option<Ipv4Addr>,
    },
    /// Show device details, name, and Wi-Fi status (registers this host).
    Info {
        /// Scanner IPv4 address.
        #[arg(long)]
        ip: Ipv4Addr,
    },
    /// Register this host and save the identity for later runs (state-changing control command 0x11).
    Register {
        /// Scanner IPv4 address.
        #[arg(long)]
        ip: Ipv4Addr,
        /// Scanner password; defaults to a saved password and then to the last four serial-number digits.
        /// An empty value explicitly selects no password.
        #[arg(long, env = PASSWORD_ENV, hide_env_values = true)]
        password: Option<String>,
    },
    /// Scan all sheets and save scanner-produced JPEG files.
    Scan {
        /// Scanner IPv4 address.
        #[arg(long)]
        ip: Ipv4Addr,
        /// Scanner password; defaults to a saved password and then to the last four serial-number digits.
        #[arg(long, env = PASSWORD_ENV, hide_env_values = true)]
        password: Option<String>,
        /// Output directory for pageNNN_front.jpg and pageNNN_back.jpg.
        #[arg(long, default_value = "scans")]
        out: PathBuf,
        /// Overwrite existing page files instead of refusing to replace them.
        #[arg(long)]
        force: bool,
        /// Color mode; grayscale host conversion is not implemented yet.
        #[arg(long, value_enum, default_value_t = ColorArg::Color)]
        mode: ColorArg,
        /// Optical resolution supported by the iX500.
        #[arg(long, default_value_t = 300)]
        dpi: u16,
        /// Keep the back-side JPEG instead of discarding it.
        #[arg(long)]
        duplex: bool,
        /// Page size used for the scan window.
        #[arg(long, default_value = "auto")]
        page: PageSize,
        /// Multi-feed detection mode.
        #[arg(long, value_enum, default_value_t = MultifeedArg::Ultrasonic)]
        multifeed: MultifeedArg,
        /// Print raw protocol details, including the raw image-info block.
        #[arg(long)]
        debug: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorArg {
    Color,
    Gray,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MultifeedArg {
    Off,
    Ultrasonic,
}

fn parse_mac_arg(value: &str) -> std::result::Result<[u8; 6], String> {
    parse_mac(value).map_err(|error| error.to_string())
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ix500(arguments) => run_ix500(arguments),
    }
}

fn run_ix500(arguments: Ix500Args) -> Result<()> {
    let mac_argument = arguments.mac;
    let timeout = Duration::from_secs(arguments.timeout);
    let save = !arguments.no_save;
    let identity_argument = arguments.identity;

    match arguments.command {
        Ix500Command::Discover { target } => {
            let route_target = target.unwrap_or(Ipv4Addr::BROADCAST);
            let mac = resolve_mac(route_target, mac_argument)?;
            let locator = Ix500Locator::new(mac);
            let target = target.map(|address| address.to_string());
            let found = locator.discover(target.as_deref(), timeout)?;
            if found.is_empty() {
                println!("No iX500 scanner found.");
            }
            for scanner in found {
                println!(
                    "{}\t{}\t{}\tserial={}",
                    scanner.address, scanner.model, scanner.name, scanner.serial
                );
            }
            Ok(())
        }
        Ix500Command::Info { ip } => {
            let mut scanner = build_scanner(ip, mac_argument, identity_argument, save, timeout)?;
            // The iX500 closes the control connection for 0x13 until a host is registered.
            let device_name = scanner.device_name()?;
            let wifi_status = scanner.wifi_status()?;
            if let Some(info) = scanner.info() {
                print_details(info);
            }
            println!("Device name: {device_name}");
            println!("Wi-Fi status: {wifi_status} (meaning unverified)");
            report_identity(&scanner);
            Ok(())
        }
        Ix500Command::Register { ip, password } => {
            let mut scanner = build_scanner(ip, mac_argument, identity_argument, save, timeout)?;
            if let Some(password) = password {
                scanner = scanner.with_password(password);
            }
            scanner.connect()?;
            if let Some(info) = scanner.info() {
                print_details(info);
            }
            println!("Registration accepted by {ip}.");
            report_identity(&scanner);
            Ok(())
        }
        Ix500Command::Scan {
            ip,
            password,
            out,
            force,
            mode,
            dpi,
            duplex,
            page,
            multifeed,
            debug,
        } => {
            let mut scanner = build_scanner(ip, mac_argument, identity_argument, save, timeout)?;
            if let Some(password) = password {
                scanner = scanner.with_password(password);
            }
            let settings = ScanSettings {
                color_mode: match mode {
                    ColorArg::Color => ColorMode::Color,
                    ColorArg::Gray => ColorMode::Gray,
                },
                dpi,
                duplex,
                page_size: page,
                multifeed: match multifeed {
                    MultifeedArg::Off => MultifeedDetection::Off,
                    MultifeedArg::Ultrasonic => MultifeedDetection::Ultrasonic,
                },
            };
            let mut output = FileScanOutput::new(out, force)?;
            let mut index = 0_u32;
            let summary = scanner.scan(&settings, &mut |sheet| {
                index += 1;
                for page in output.write(index, &sheet)? {
                    println!("Sheet {index}: {}", page.path.display());
                }
                if let Some(info) = &sheet.info {
                    println!("  {}", describe(info));
                    if debug {
                        println!("  raw image info: {}", format_hex(&info.raw));
                    }
                }
                Ok(())
            })?;
            scanner.disconnect()?;
            println!(
                "Scanned {} sheet(s), saved {} image(s).",
                summary.sheets, summary.images
            );
            report_identity(&scanner);
            Ok(())
        }
    }
}

fn build_scanner(
    ip: Ipv4Addr,
    mac: Option<[u8; 6]>,
    identity_path: Option<PathBuf>,
    save: bool,
    timeout: Duration,
) -> Result<Ix500Scanner> {
    let mac = resolve_mac(ip, mac)?;
    let path = match identity_path {
        Some(path) => path,
        None => identity::default_path()?,
    };
    Ok(Ix500Scanner::new(ip, mac)
        .with_identity(path, save)
        .with_discovery_timeout(timeout))
}

fn resolve_mac(target: Ipv4Addr, provided: Option<[u8; 6]>) -> Result<[u8; 6]> {
    provided.map_or_else(
        || local_identity_for(target).map(|identity| identity.mac),
        Ok,
    )
}

fn report_identity(scanner: &Ix500Scanner) {
    if scanner.identity_saved() {
        println!(
            "Identity and password saved to {} (mode 0600).",
            scanner.identity_path().display()
        );
    } else if let Some(error) = scanner.identity_save_error() {
        eprintln!("warning: identity was not saved: {error}");
    }
}

fn describe(info: &ImageInfo) -> String {
    let resolution = if info.dpi_x > 0 && info.dpi_y > 0 {
        format!(" at {}x{} dpi", info.dpi_x, info.dpi_y)
    } else {
        format!(
            "; scanner reported resolution fields {}x{} dpi",
            info.dpi_x, info.dpi_y
        )
    };
    format!(
        "{}x{} pixels{resolution} (raw scanner orientation)",
        info.width, info.height
    )
}

fn print_details(info: &ScannerInfo) {
    println!("Model: {}", info.model);
    println!("Name: {}", info.name);
    println!("Serial: {}", info.serial);
    println!("Address: {}", info.ip);
    println!("Scanner MAC: {}", info.mac_string());
    println!("Interface version: {}", info.interface_version);
    println!("Paired host: {}", info.paired_ip);
    println!("Paired host ID: {}", info.paired_host_id_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ix500_commands_and_shared_options() {
        assert!(
            Cli::try_parse_from([
                "ferriscan",
                "ix500",
                "--mac",
                "02:00:00:00:00:01",
                "discover",
                "--target",
                "192.0.2.20",
            ])
            .is_ok()
        );

        assert!(
            Cli::try_parse_from([
                "ferriscan",
                "ix500",
                "--identity",
                "/tmp/ix500.conf",
                "--no-save",
                "register",
                "--ip",
                "192.0.2.20",
                "--password",
                "0000",
            ])
            .is_ok()
        );

        assert!(
            Cli::try_parse_from([
                "ferriscan",
                "ix500",
                "scan",
                "--ip",
                "192.0.2.20",
                "--dpi",
                "300",
                "--duplex",
                "--page",
                "a4",
                "--force",
                "--debug",
            ])
            .is_ok()
        );
    }

    #[test]
    fn rejects_removed_sheet_limit_options() {
        for arguments in [
            vec!["--max-sheets=1"],
            vec!["--max-sheets", "1"],
            vec!["--max_sheets=2"],
        ] {
            let mut cli_arguments = vec!["ferriscan", "ix500", "scan", "--ip", "192.0.2.20"];
            cli_arguments.extend(arguments);
            assert!(Cli::try_parse_from(cli_arguments).is_err());
        }
    }

    #[test]
    fn rejects_invalid_addresses_and_unknown_commands() {
        assert!(
            Cli::try_parse_from(["ferriscan", "ix500", "--mac", "invalid", "discover"]).is_err()
        );
        assert!(Cli::try_parse_from(["ferriscan", "unknown"]).is_err());
    }

    #[test]
    fn describes_pages_with_and_without_usable_resolution_fields() {
        let with_dpi = ImageInfo {
            width: 2608,
            max_height: 4400,
            height: 3664,
            dpi_x: 300,
            dpi_y: 300,
            raw: Vec::new(),
        };
        assert_eq!(
            describe(&with_dpi),
            "2608x3664 pixels at 300x300 dpi (raw scanner orientation)"
        );

        let without_dpi = ImageInfo {
            dpi_x: 0,
            ..with_dpi
        };
        assert_eq!(
            describe(&without_dpi),
            "2608x3664 pixels; scanner reported resolution fields 0x300 dpi (raw scanner orientation)"
        );
    }

    #[test]
    fn formats_raw_blocks_as_lowercase_hex() {
        assert_eq!(format_hex(&[0x00, 0x0a, 0xff]), "000aff");
    }
}
