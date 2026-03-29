use clap::Parser;
use chrono::Utc;
use log::{info, error};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Parser)]
#[command(name = "music-browser-backup")]
#[command(about = "Backup utility for PersonalMusicBrowser database")]
struct Args {
    /// Database file path
    #[arg(short, long, default_value = "music_browser.db")]
    db_file: String,

    /// Backup directory (overrides BACKUP_DIR env var)
    #[arg(short, long)]
    backup_dir: Option<String>,

    /// SMB path for remote backup (overrides BACKUP_SMB_PATH env var)
    #[arg(short = 's', long)]
    smb_path: Option<String>,

    /// Skip SMB backup
    #[arg(long)]
    no_smb: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let args = Args::parse();
    
    // Load environment variables
    dotenvy::dotenv().ok();
    
    let backup_dir = args.backup_dir
        .or_else(|| env::var("BACKUP_DIR").ok())
        .unwrap_or_else(|| shellexpand::tilde("~").to_string());
    
    let smb_path = if args.no_smb {
        None
    } else {
        args.smb_path.or_else(|| env::var("BACKUP_SMB_PATH").ok())
    };

    // Generate timestamped backup filename
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_filename = format!("music_browser_backup_{}.db", timestamp);
    let backup_path = Path::new(&backup_dir).join(&backup_filename);

    info!("Creating backup: {}", backup_path.display());

    // Create backup
    fs::copy(&args.db_file, &backup_path)
        .map_err(|e| format!("Failed to copy database: {}", e))?;

    // Try SMB backup if configured
    if let Some(smb) = &smb_path {
        info!("Attempting SMB backup to: {}", smb);
        
        // Use smbclient command (requires samba-client package)
        let output = Command::new("smbclient")
            .arg(smb)
            .arg("-c")
            .arg(&format!("put {}", backup_filename))
            .current_dir(&backup_dir)
            .output();

        match output {
            Ok(result) if result.status.success() => {
                info!("SMB backup successful");
            }
            Ok(_) => {
                error!("SMB backup failed, keeping local backup only");
                let stderr = String::from_utf8_lossy(&output.stdout);
                error!("smbclient error: {}", stderr);
            }
            Err(e) => {
                error!("Failed to run smbclient: {}", e);
                error!("Make sure samba-client is installed for SMB backups");
            }
        }
    }

    info!("Backup completed: {}", backup_path.display());
    Ok(())
}
