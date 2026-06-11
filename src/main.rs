// src/main.rs
// SysWatch — Moniteur système TCP multi-threadé
// Étapes 1 à 5 : types, collecte, formatage, serveur TCP, journalisation

use chrono::Local;
use std::fmt;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use sysinfo::{Process, System};

/// Token d'authentification requis à chaque connexion TCP
const AUTH_TOKEN: &str = "ENSPD2026";

// =============================================================================
// ÉTAPE 1 — Modélisation des données
// Concepts : struct, impl, trait Display, Vec<T>, derive(Debug, Clone)
// =============================================================================

#[derive(Debug, Clone)]
struct CpuInfo {
    usage_percent: f32,
    core_count: usize,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_usage: f32,
    memory_mb: u64,
}

#[derive(Debug, Clone)]
struct SystemSnapshot {
    timestamp: String,
    cpu: CpuInfo,
    memory: MemInfo,
    top_processes: Vec<ProcessInfo>,
}

// --- Trait Display : affichage humain ---

impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CPU: {:.1}% ({} cœurs)",
            self.usage_percent, self.core_count
        )
    }
}

impl fmt::Display for MemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MEM: {}MB utilisés / {}MB total ({} MB libres)",
            self.used_mb, self.total_mb, self.free_mb
        )
    }
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  [{:>6}] {:<25} CPU:{:>5.1}%  MEM:{:>5}MB",
            self.pid, self.name, self.cpu_usage, self.memory_mb
        )
    }
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SysWatch — {} ===", self.timestamp)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        writeln!(f, "--- Top Processus ---")?;
        for p in &self.top_processes {
            writeln!(f, "{}", p)?;
        }
        write!(f, "=====================")
    }
}

// =============================================================================
// ÉTAPE 2 — Collecte réelle et gestion d'erreurs
// Concepts : Result<T, E>, enum d'erreur personnalisée, closures, .map(), .sort_by()
// =============================================================================

/// Erreurs possibles lors de la collecte des métriques système
#[derive(Debug)]
enum SysWatchError {
    CollectionFailed(String),
}

impl fmt::Display for SysWatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SysWatchError::CollectionFailed(msg) => write!(f, "Erreur collecte: {}", msg),
        }
    }
}

impl std::error::Error for SysWatchError {}

/// Collecte un instantané des métriques système (CPU, RAM, top 5 processus).
/// Retourne une erreur si aucun CPU n'est détecté.
fn collect_snapshot() -> Result<SystemSnapshot, SysWatchError> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Petite pause pour que sysinfo produise des valeurs CPU non nulles
    thread::sleep(Duration::from_millis(500));
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let core_count = sys.cpus().len();

    if core_count == 0 {
        return Err(SysWatchError::CollectionFailed(
            "Aucun CPU détecté".to_string(),
        ));
    }

    let total_mb = sys.total_memory() / 1024 / 1024;
    let used_mb = sys.used_memory() / 1024 / 1024;
    let free_mb = sys.free_memory() / 1024 / 1024;

    // Collecte des processus, tri par CPU décroissant, garde le top 5
    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p: &Process| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage(),
            memory_mb: p.memory() / 1024 / 1024,
        })
        .collect();

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes.truncate(5);

    Ok(SystemSnapshot {
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        cpu: CpuInfo {
            usage_percent: cpu_usage,
            core_count,
        },
        memory: MemInfo {
            total_mb,
            used_mb,
            free_mb,
        },
        top_processes: processes,
    })
}

// =============================================================================
// ÉTAPE 3 — Formatage des réponses réseau
// Concepts : pattern matching exhaustif sur &str, itérateurs, barres ASCII
// =============================================================================

/// Formate la réponse à envoyer au client en fonction de la commande reçue.
/// Commandes supportées : cpu, mem, ps/procs, all, help, quit, msg, install,
///                        shutdown, reboot, abort.
fn format_response(snapshot: &SystemSnapshot, command: &str) -> String {
    let cmd = command.trim().to_lowercase();

    match cmd.as_str() {
        // --- Vue CPU avec barre ASCII ---
        "cpu" => {
            let filled = (snapshot.cpu.usage_percent / 10.0) as usize;
            let bar: String = (0..10)
                .map(|i| if i < filled { "█" } else { "░" })
                .collect::<Vec<_>>()
                .join("");
            format!(
                "[CPU]\n{}\nCharge : {} {:.1}%\n",
                snapshot.cpu, bar, snapshot.cpu.usage_percent
            )
        }

        // --- Vue RAM avec barre sur 20 cases ---
        "mem" => {
            let percent = (snapshot.memory.used_mb as f64
                / snapshot.memory.total_mb as f64)
                * 100.0;
            let bar: String = (0..20)
                .map(|i| if i < (percent / 5.0) as usize { '█' } else { '░' })
                .collect();
            format!(
                "[MÉMOIRE]\n{}\n[{}] {:.1}%\n",
                snapshot.memory, bar, percent
            )
        }

        // --- Liste des processus ---
        "ps" | "procs" => {
            let lines: String = snapshot
                .top_processes
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}. {}", i + 1, p))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "[PROCESSUS — Top {}]\n{}\n",
                snapshot.top_processes.len(),
                lines
            )
        }

        // --- Vue complète ---
        "all" | "" => format!("{}\n", snapshot),

        // --- Aide ---
        "help" => concat!(
            "Commandes disponibles:\n",
            "  cpu      — Usage CPU + barre ASCII\n",
            "  mem      — Utilisation mémoire RAM\n",
            "  ps       — Top 5 processus (CPU)\n",
            "  all      — Vue système complète\n",
            "  help     — Cette aide\n",
            "  quit     — Fermer la connexion\n",
            "  msg <X>  — Afficher un message sur la machine cible\n",
            "  install <pkg> — Installer un paquet via winget\n",
            "  shutdown — Éteindre la machine dans 5 s\n",
            "  reboot   — Redémarrer la machine dans 5 s\n",
            "  abort    — Annuler un shutdown/reboot en cours\n",
        )
        .to_string(),

        // --- Déconnexion propre ---
        "quit" | "exit" => "BYE\n".to_string(),

        // --- Commandes système (Windows uniquement) ---
        "shutdown" => {
            std::process::Command::new("shutdown")
                .args(["/s", "/t", "5"])
                .spawn()
                .ok();
            "SHUTDOWN programmé dans 5 secondes.\n".to_string()
        }

        "reboot" => {
            std::process::Command::new("shutdown")
                .args(["/r", "/t", "5"])
                .spawn()
                .ok();
            "REBOOT programmé dans 5 secondes.\n".to_string()
        }

        "abort" => {
            std::process::Command::new("shutdown")
                .args(["/a"])
                .spawn()
                .ok();
            "Extinction annulée.\n".to_string()
        }

        // --- Message affiché dans le terminal local ---
        _ if cmd.starts_with("msg ") => {
            let text = &cmd[4..];
            println!("\n╔══════════════════════════════════════╗");
            println!("║  MESSAGE DU PROFESSEUR               ║");
            println!(
                "║  {}{}║",
                text,
                " ".repeat(38usize.saturating_sub(text.len()))
            );
            println!("╚══════════════════════════════════════╝\n");
            "Message affiché sur la machine cible.\n".to_string()
        }

        // --- Installation silencieuse via winget ---
        _ if cmd.starts_with("install ") => {
            let package = cmd[8..].trim().to_string();
            thread::spawn(move || {
                std::process::Command::new("winget")
                    .args(["install", "--silent", &package])
                    .status()
                    .ok();
            });
            format!("Installation de '{}' lancée en arrière-plan.\n", &cmd[8..])
        }

        // --- Commande inconnue ---
        _ => format!("Commande inconnue : '{}'. Tape 'help'.\n", command.trim()),
    }
}

// =============================================================================
// ÉTAPE 4 — Serveur TCP multi-threadé
// Concepts : TcpListener, TcpStream, thread::spawn, Arc<Mutex<T>>
// =============================================================================

/// Gère un client TCP dans son propre thread.
/// Protocole :
///   1. Le serveur envoie « TOKEN: »
///   2. Le client répond avec le token (AUTH_TOKEN)
///   3. Si correct → boucle de commandes ; sinon → UNAUTHORIZED et fermeture
///   4. Chaque réponse est terminée par le marqueur « \nEND\n »
fn handle_client(mut stream: TcpStream, snapshot: Arc<Mutex<SystemSnapshot>>) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "inconnu".to_string());
    log_event(&format!("[+] Connexion de {}", peer));

    // ── Authentification ──────────────────────────────────────────────────────
    let _ = stream.write_all(b"TOKEN: ");

    let mut reader = BufReader::new(stream.try_clone().expect("Impossible de cloner le stream"));
    let mut token_line = String::new();

    if reader.read_line(&mut token_line).is_err() || token_line.trim() != AUTH_TOKEN {
        let _ = stream.write_all(b"UNAUTHORIZED\n");
        log_event(&format!("[!] Acces refuse depuis {}", peer));
        return; // fermeture automatique du stream en fin de portée
    }

    let _ = stream.write_all(b"OK\n");
    log_event(&format!("[OK] Authentifie : {}", peer));

    // Bannière de bienvenue
    let banner = concat!(
        "╔══════════════════════════════════╗\n",
        "║   SysWatch v1.0 — ENSPD 2026     ║\n",
        "║   Tape 'help' pour les commandes ║\n",
        "╚══════════════════════════════════╝\n",
    );
    let _ = stream.write_all(banner.as_bytes());
    let _ = stream.write_all(b"\nEND\n");

    // ── Boucle de commandes ───────────────────────────────────────────────────
    for line in reader.lines() {
        match line {
            Ok(cmd) => {
                let cmd = cmd.trim().to_string();
                log_event(&format!("[{}] commande : '{}'", peer, cmd));

                // Déconnexion propre
                if cmd.eq_ignore_ascii_case("quit") || cmd.eq_ignore_ascii_case("exit") {
                    let _ = stream.write_all(b"Au revoir !\n\nEND\n");
                    break;
                }

                // Lecture thread-safe du snapshot partagé
                let response = {
                    let snap = snapshot.lock().unwrap();
                    format_response(&snap, &cmd)
                };

                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(b"\nEND\n"); // marqueur de fin de réponse
            }
            Err(_) => break, // déconnexion brutale
        }
    }

    log_event(&format!("[-] Deconnexion de {}", peer));
}

/// Thread de rafraîchissement : met à jour le snapshot partagé toutes les 5 s.
fn snapshot_refresher(snapshot: Arc<Mutex<SystemSnapshot>>) {
    loop {
        thread::sleep(Duration::from_secs(5));
        match collect_snapshot() {
            Ok(new_snap) => {
                let mut snap = snapshot.lock().unwrap();
                *snap = new_snap;
                log_event("[refresh] Metriques mises a jour");
            }
            Err(e) => eprintln!("[refresh] Erreur : {}", e),
        }
    }
}

// =============================================================================
// ÉTAPE 5 — Journalisation fichier (bonus)
// Concepts : OpenOptions, mode append, std::io::Write
// =============================================================================

/// Écrit un événement horodaté sur stdout ET dans le fichier `syswatch.log`.
/// L'écriture fichier est best-effort : une erreur d'I/O est silencieusement ignorée.
fn log_event(message: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let line = format!("[{}] {}\n", timestamp, message);

    // Affichage console (flush explicite : stdout est bufferisé dans Docker)
    print!("{}", line);
    let _ = std::io::stdout().flush();

    // Écriture en mode append dans le fichier journal
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("syswatch.log")
    {
        let _ = file.write_all(line.as_bytes());
    }
}

// =============================================================================
// POINT D'ENTRÉE
// =============================================================================

fn main() {
    println!("SysWatch démarrage...");

    // ── Collecte initiale ─────────────────────────────────────────────────────
    let initial = collect_snapshot().expect("Impossible de collecter les métriques initiales");
    println!("Métriques initiales :\n{}", initial);
    log_event("Demarrage SysWatch");

    // Snapshot partagé via Arc<Mutex<T>> entre tous les threads
    let shared_snapshot = Arc::new(Mutex::new(initial));

    // ── Thread de rafraîchissement automatique ────────────────────────────────
    {
        let snap_clone = Arc::clone(&shared_snapshot);
        thread::spawn(move || snapshot_refresher(snap_clone));
    }

    // ── Serveur TCP ───────────────────────────────────────────────────────────
    let listener =
        TcpListener::bind("0.0.0.0:7878").expect("Impossible de bind le port 7878");

    log_event("Serveur TCP en ecoute sur 0.0.0.0:7878");
    println!("Connecte-toi avec :");
    println!("  telnet localhost 7878");
    println!("  nc localhost 7878          (WSL / Git Bash)");
    println!("Ctrl+C pour arrêter.\n");

    // Boucle d'acceptation : chaque connexion → thread dédié
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let snap_clone = Arc::clone(&shared_snapshot);
                thread::spawn(move || handle_client(stream, snap_clone));
            }
            Err(e) => eprintln!("Erreur connexion entrante : {}", e),
        }
    }
}