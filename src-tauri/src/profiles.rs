//! Profile für das lokale Terminal: installierte Shells und Sprach-Interpreter erkennen
//! (wie Windows Terminal). Nur lesen und Programme suchen, nichts verändern.

use serde::Serialize;
use std::path::PathBuf;
#[cfg(windows)]
use std::path::Path;

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

/// Ordner, in denen Programme liegen: PATH und dazu die üblichen Installationsorte. Aus dem Finder,
/// dem Startmenü oder dem Dock gestartete Apps erben nicht den PATH der Login-Shell
/// (unter macOS fehlen z. B. /opt/homebrew/bin und /usr/local/microsoft/powershell/7).
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if cfg!(windows) {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let pf = env("ProgramFiles").unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        dirs.extend([pf.join(r"PowerShell\7"), pf.join(r"nu\bin"), pf.join(r"Git\bin")]);
        if let Some(l) = env("LOCALAPPDATA") {
            dirs.extend([l.join(r"Microsoft\WinGet\Links"), l.join(r"Programs\nu\bin")]);
        }
        if let Some(h) = &home {
            dirs.extend([h.join(r"scoop\shims"), h.join(r".cargo\bin")]);
        }
    } else {
        // Was path_helper (macOS) der Login-Shell gibt: /etc/paths und /etc/paths.d/* (dort trägt sich z. B. PowerShell ein).
        let mut files = vec![PathBuf::from("/etc/paths")];
        if let Ok(rd) = std::fs::read_dir("/etc/paths.d") {
            let mut more: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            more.sort();
            files.extend(more);
        }
        for f in files {
            if let Ok(t) = std::fs::read_to_string(f) {
                dirs.extend(t.lines().map(str::trim).filter(|l| l.starts_with('/')).map(PathBuf::from));
            }
        }
        for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/local/microsoft/powershell/7", "/opt/microsoft/powershell/7", "/snap/bin", "/home/linuxbrew/.linuxbrew/bin", "/run/current-system/sw/bin", "/opt/local/bin", "/usr/bin", "/bin"] {
            dirs.push(PathBuf::from(d));
        }
        if let Some(h) = &home {
            dirs.extend([h.join(".local/bin"), h.join(".cargo/bin"), h.join(".nix-profile/bin"), h.join("bin")]);
        }
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// Programm suchen (unter Windows mit .exe/.cmd/.bat), im PATH und an den üblichen Installationsorten.
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, &search_dirs())
}

fn which_in(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    for dir in dirs {
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

#[cfg_attr(not(windows), allow(dead_code))]
/// Ausgabe von `wsl.exe -l -q` (UTF-16LE, teils mit BOM/NUL) → Namen der Distributionen.
pub fn parse_wsl_list(bytes: &[u8]) -> Vec<String> {
    let text = if bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).take(8).all(|&b| b == 0) || bytes.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
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
    if let Some(nu) = which("nu") {
        out.push(p("nu", "Nushell", nu.to_string_lossy(), &[], "shell"));
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
    let dirs = search_dirs();
    // Login-Shell zuerst (Vorschlag für «Standard»), dann /etc/shells, dann bekannte Shells an den üblichen Orten.
    let mut shells: Vec<PathBuf> = std::env::var("SHELL").ok().map(PathBuf::from).into_iter().collect();
    if let Ok(list) = std::fs::read_to_string("/etc/shells") {
        shells.extend(list.lines().map(str::trim).filter(|l| l.starts_with('/')).map(PathBuf::from));
    }
    shells.extend(UNIX_SHELLS.iter().filter_map(|(bin, ..)| which_in(bin, &dirs)));
    for s in shells {
        let bin = s.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(&(_, name, args, icon)) = UNIX_SHELLS.iter().find(|(b, ..)| *b == bin) else { continue };
        if !s.is_file() || out.iter().any(|x| x.id == bin) {
            continue;
        }
        out.push(p(&bin, name, s.to_string_lossy(), args, icon));
    }
}

/// Bekannte Shells: Programmname, Anzeigename, Argumente (als Login-Shell, damit Profil und PATH stimmen), Symbol.
#[cfg(not(windows))]
const UNIX_SHELLS: &[(&str, &str, &[&str], &str)] = &[
    ("zsh", "zsh", &["-l"], "shell"),
    ("bash", "bash", &["-l"], "shell"),
    ("fish", "fish", &["-l"], "shell"),
    ("nu", "Nushell", &["-l"], "shell"),
    ("pwsh", "PowerShell", &["-NoLogo", "-Login"], "powershell"),
    ("xonsh", "xonsh", &["-l"], "shell"),
    ("elvish", "Elvish", &[], "shell"),
    ("sh", "sh", &["-l"], "shell"),
];

/// Alle erkannten Profile; die erste Shell ist der Vorschlag für «Standard».
pub fn detect() -> Vec<Profile> {
    let mut out = Vec::new();
    if cfg!(mobile) {
        return out; // keine lokalen Programme auf iPadOS/Android
    }
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
    fn finds_programs_outside_path() {
        // Wie eine aus dem Finder gestartete App: PATH ohne Installationsorte.
        let dir = std::env::temp_dir().join(format!("sesam-which-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "pwsh.exe" } else { "pwsh" });
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(which_in("pwsh", &[PathBuf::from("/nirgends"), dir.clone()]), Some(exe));
        assert_eq!(which_in("pwsh", &[PathBuf::from("/nirgends")]), None);
        std::fs::remove_dir_all(&dir).unwrap();
        // Die üblichen Orte gehören immer dazu, auch ohne PATH.
        let dirs = search_dirs();
        let want = if cfg!(windows) { "PowerShell" } else { "/usr/local/microsoft/powershell/7" };
        assert!(dirs.iter().any(|d| d.to_string_lossy().contains(want)), "{dirs:?}");
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
