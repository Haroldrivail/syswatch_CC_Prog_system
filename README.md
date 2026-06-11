# SysWatch

Moniteur système réseau multi-threadé écrit en Rust, développé dans le cadre du cours de programmation système à l'**ENSPD 2026**.

## Présentation

SysWatch permet à un professeur de surveiller en temps réel les machines de ses étudiants depuis un poste central. Chaque machine étudiante fait tourner un **agent** (serveur TCP) qui collecte les métriques système. Le professeur utilise le **master** (client interactif) pour interroger les agents et envoyer des commandes à distance.

```
[PC Étudiant 1] ──┐
[PC Étudiant 2] ──┤── réseau local ──► [PC Professeur - Master]
[PC Étudiant 3] ──┘
```

## Architecture

| Binaire | Fichier source | Rôle |
|---|---|---|
| `syswatch` | `src/main.rs` | Agent — tourne sur chaque PC étudiant |
| `syswatch-master` | `src/master.rs` | Master — tourne sur le PC du professeur |

### Agent (`syswatch`)

- Écoute sur le port **7878**
- Authentifie chaque connexion avec un token (`ENSPD2026`)
- Collecte CPU, RAM et top 5 processus via la crate `sysinfo`
- Répond aux commandes du master et journalise les événements dans un fichier log

### Master (`syswatch-master`)

- Se connecte aux agents listés dans `src/master.rs`
- Propose un shell interactif pour piloter les machines à distance

## Dépendances

| Crate | Version | Utilisation |
|---|---|---|
| `sysinfo` | 0.30 | Lecture des métriques système (CPU, RAM, processus) |
| `chrono` | 0.4 | Horodatage des instantanés |

## Contributeurs

| Nom | Machine | IP |
|---|---|---|
| TSEFACK | PC-01 | 192.168.1.101 |
| FOKAM | PC-02 | 192.168.1.102 |
| NZEUTEM | PC-03 | 192.168.1.103 |
| ATEBA | PC-04 | 192.168.1.105 |

> Pour ajouter une machine, modifier la fonction `machines()` dans `src/master.rs`.

## Compilation

```bash
# Compiler les deux binaires
cargo build --release

# Ou lancer directement en mode debug
cargo run --bin syswatch          # agent
cargo run --bin syswatch-master   # master
```

## Utilisation

### 1. Lancer l'agent sur chaque PC étudiant

```bash
cargo run --bin syswatch
```

L'agent démarre et attend les connexions sur le port 7878.

### 2. Lancer le master sur le PC du professeur

```bash
cargo run --bin syswatch-master
```

### 3. Commandes du master

```
scan              — lister toutes les machines et leur état (en ligne / hors ligne)
select <nom>      — cibler une machine (ex: select PC-01-TSEFACK)
all <commande>    — envoyer une commande à toutes les machines
help              — afficher l'aide
quit              — quitter le master
```

### 4. Commandes disponibles sur les agents

```
cpu               — usage CPU avec barre ASCII
mem               — utilisation de la RAM
ps                — top 5 processus par consommation CPU
all               — vue système complète
msg <texte>       — afficher un message sur la machine cible
install <paquet>  — installer un logiciel via winget (Windows)
shutdown          — éteindre la machine dans 5 secondes
reboot            — redémarrer la machine dans 5 secondes
abort             — annuler un shutdown/reboot en cours
```

### Exemple de session

```
[master]> scan
  PC-01-TSEFACK (192.168.1.101) — ✓ EN LIGNE
  PC-02-FOKAM   (192.168.1.102) — ✓ EN LIGNE
  PC-03-NZEUTEM (192.168.1.103) — ✗ HORS LIGNE

[master]> select PC-01-TSEFACK
Machine sélectionnée : PC-01-TSEFACK

[master@PC-01-TSEFACK]> cpu
[CPU]
CPU: 12.4% (8 cœurs)
Charge : [█░░░░░░░░░] 12.4%

[master@PC-01-TSEFACK]> all ps
Envoi de 'ps' à toutes les machines...
```
