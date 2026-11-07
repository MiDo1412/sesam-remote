//! Profile für das lokale Terminal: installierte Shells und Sprach-Interpreter erkennen
//! (wie Windows Terminal). Nur lesen und Programme suchen, nichts verändern.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    /// Symbol im Frontend: "powershell", "cmd", "linux", "git", "vs", "python", "node", "ruby", "lua", "shell".
    pub icon: String,
}

fn p(id: &str, name: &str, command: impl Into<String>, args: &[&str], icon: &str) -> Profile {
    Profile { id: id.into(), name: name.into(), command: command.into(), args: args.iter().map(|s| s.to_string()).collect(), icon: icon.into() }
}

/// Programm im PATH suchen (unter Windows mit .exe/.cmd/.bat).
pub fn which(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        // Windows-Store-Platzhalter für python.exe öffnen nur den Store: überspringen.
        if dir.to_string_lossy().contains("WindowsApps") {
            continue;
        }
        for ext in exts {
            let f = dir.join(format!("{name}{ext}"));
            if f.is_file() {
                return Some(f);
            }
        }
    }
    None
}

/// Ausgabe von `wsl.exe -l -q` (UTF-16LE, teils mit BOM/NUL) → Namen der Distributionen.
pub fn parse_wsl_list(bytes: &[u8]) -> Vec<String> {
    let text = if bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).take(8).all(|&b| b == 0) || bytes.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0').to_string())
        .filter(|l| !l.is_empty() && !l.starts_with("docker-desktop"))
        .collect()
}

#[cfg(windows)]
fn windows_profiles(out: &mut Vec<Profile>) {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let pwsh = which("pwsh").or_else(|| Some(PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe")).filter(|p| p.is_file()));
    if let Some(pwsh) = pwsh {
        out.push(p("pwsh", "PowerShell", pwsh.to_string_lossy(), &["-NoLogo"], "powershell"));
    }
    let ps = Path::new(&sys).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if ps.is_file() {
        out.push(p("powershell", "Windows PowerShell", ps.to_string_lossy(), &["-NoLogo"], "powershell"));
    }
    let cmd = std::env::var("ComSpec").unwrap_or_else(|_| format!(r"{sys}\System32\cmd.exe"));
    out.push(p("cmd", "Eingabeaufforderung", cmd.clone(), &[], "cmd"));
    let wsl = Path::new(&sys).join(r"System32\wsl.exe");
    if wsl.is_file() {
        if let Ok(o) = std::process::Command::new(&wsl).args(["-l", "-q"]).creation_flags(NO_WINDOW).output() {
            for d in parse_wsl_list(&o.stdout) {
                out.push(Profile { id: format!("wsl-{d}"), name: d.clone(), command: wsl.to_string_lossy().into(), args: vec!["-d".into(), d], icon: "linux".into() });
            }
        }
    }
    for git in [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files (x86)\Git\bin\bash.exe"] {
        if Path::new(git).is_file() {
            out.push(p("git-bash", "Git Bash", git, &["--login", "-i"], "git"));
            break;
        }
    }
    // Visual Studio: Developer-Shells über vswhere (wie Windows Terminal).
    let vswhere = r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe";
    if Path::new(vswhere).is_file() {
        if let Ok(o) = std::process::Command::new(vswhere).args(["-latest", "-property", "installationPath"]).creation_flags(NO_WINDOW).output() {
            let vs = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let bat = format!(r"{vs}\Common7\Tools\VsDevCmd.bat");
            if !vs.is_empty() && Path::new(&bat).is_file() {
                out.push(Profile { id: "vs-cmd".into(), name: "Developer Command Prompt".into(), command: cmd, args: vec!["/k".into(), bat], icon: "vs".into() });
                let dll = format!(r"{vs}\Common7\Tools\Microsoft.VisualStudio.DevShell.dll");
                if Path::new(&dll).is_file() {
                    out.push(Profile {
                        id: "vs-pwsh".into(),
                        name: "Developer PowerShell".into(),
                        command: ps.to_string_lossy().into(),
                        args: vec!["-NoExit".into(), "-Command".into(), format!("&{{Import-Module '{dll}'; Enter-VsDevShell -VsInstallPath '{vs}' -SkipAutomaticLocation}}")],
                        icon: "vs".into(),
                    });
                }
            }
        }
    }
}

#[cfg(not(windows))]
fn unix_profiles(out: &mut Vec<Profile>) {
    let login = std::env::var("SHELL").ok();
    let mut shells: Vec<String> = login.iter().cloned().collect();
    if let Ok(list) = std::fs::read_to_string("/etc/shells") {
        shells.extend(list.lines().map(str::trim).filter(|l| l.starts_with('/')).map(String::from));
    }
    for s in shells {
        let name = Path::new(&s).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !["zsh", "bash", "fish", "sh"].contains(&name.as_str()) || !Path::new(&s).is_file() || out.iter().any(|x| x.id == name) {
            continue;
        }
        out.push(Profile { id: name.clone(), name: name.clone(), command: s, args: vec!["-l".into()], icon: "shell".into() });
    }
}

/// Alle erkannten Profile; die erste Shell ist der Vorschlag für «Standard».
pub fn detect() -> Vec<Profile> {
    let mut out = Vec::new();
    #[cfg(windows)]
    windows_profiles(&mut out);
    #[cfg(not(windows))]
    unix_profiles(&mut out);
    // Sprach-Interpreter, falls installiert.
    for (id, name, bins, icon) in [
        ("python", "Python", &["python3", "python"][..], "python"),
        ("node", "Node.js", &["node"][..], "node"),
        ("ruby", "Ruby (irb)", &["irb"][..], "ruby"),
        ("lua", "Lua", &["lua"][..], "lua"),
    ] {
        if let Some(path) = bins.iter().find_map(|b| which(b)) {
            out.push(Profile { id: id.into(), name: name.into(), command: path.to_string_lossy().into(), args: Vec::new(), icon: icon.into() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_list_utf16_and_utf8() {
        let mut b = vec![0xFF, 0xFE];
        for u in "Ubuntu\r\ndocker-desktop\r\nDebian\r\n".encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(parse_wsl_list(&b), vec!["Ubuntu", "Debian"]);
        assert_eq!(parse_wsl_list(b"Ubuntu-24.04\n\n"), vec!["Ubuntu-24.04"]);
        assert!(parse_wsl_list(b"").is_empty());
    }

    #[test]
    fn detect_finds_a_shell() {
        let all = detect();
        for p in &all {
            println!("{} · {} {:?}", p.name, p.command, p.args);
        }
        assert!(!all.is_empty(), "mindestens eine Shell");
        let ids: Vec<_> = all.iter().map(|p| &p.id).collect();
        assert_eq!(ids.len(), ids.iter().collect::<std::collections::HashSet<_>>().len(), "IDs eindeutig");
    }
}
