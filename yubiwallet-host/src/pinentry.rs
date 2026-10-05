//! Local PIN entry. Child stdio is isolated from native-messaging stdout.
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};

pub(crate) fn read_pin() -> Result<String, String> {
    if let Ok(pin) = std::env::var("YUBIWALLET_PIN") {
        return nonempty(pin);
    }
    match std::env::var("YUBIWALLET_PIN_MODE").as_deref() {
        Ok("tty") => tty_pin(),
        Ok("gui") | Err(_) => {
            let executable =
                std::env::var_os("YUBIWALLET_PINENTRY").unwrap_or_else(|| "pinentry-qt".into());
            prompt(&mut Command::new(executable))
        }
        _ => Err("YUBIWALLET_PIN_MODE must be gui or tty".into()),
    }
}

fn nonempty(pin: String) -> Result<String, String> {
    if pin.is_empty() {
        Err("PIN entry was empty".into())
    } else {
        Ok(pin)
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn prompt(command: &mut Command) -> Result<String, String> {
    let child = command.stdin(Stdio::piped()).stdout(Stdio::piped())
        .stderr(Stdio::null()).spawn()
        .map_err(|_| "Could not launch GUI pinentry. Install pinentry-qt or pinentry-gnome3 and set YUBIWALLET_PINENTRY to its executable path.".to_string())?;
    let mut process = Process(child);
    let mut input = process.0.stdin.take().ok_or("Cannot open pinentry input")?;
    let mut output = BufReader::new(
        process
            .0
            .stdout
            .take()
            .ok_or("Cannot open pinentry output")?,
    );
    response(&mut output, false)?;
    for command in [
        "SETTITLE YubiWallet",
        "SETDESC Enter your hardware wallet PIN locally. The PIN is not sent to the browser.",
        "SETPROMPT PIN:",
    ] {
        writeln!(input, "{command}")
            .and_then(|_| input.flush())
            .map_err(|_| "Cannot write to pinentry")?;
        response(&mut output, false)?;
    }
    writeln!(input, "GETPIN")
        .and_then(|_| input.flush())
        .map_err(|_| "Cannot request PIN")?;
    let pin = nonempty(response(&mut output, true)?)?;
    let _ = writeln!(input, "BYE");
    let _ = input.flush();
    Ok(pin)
}

fn response(reader: &mut impl BufRead, collect: bool) -> Result<String, String> {
    let mut data = Vec::new();
    loop {
        let mut line = Vec::new();
        (&mut *reader)
            .take(4097)
            .read_until(b'\n', &mut line)
            .map_err(|_| "Cannot read pinentry response")?;
        if line.is_empty() || line.len() > 4096 || !line.ends_with(b"\n") {
            return Err("Invalid or closed pinentry response".into());
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line == b"OK" || line.starts_with(b"OK ") {
            return String::from_utf8(data).map_err(|_| "PIN is not valid UTF-8".into());
        }
        if line.starts_with(b"ERR ") {
            // Do not echo the child response: it may contain sensitive data.
            let code = line
                .split(|b| *b == b' ')
                .nth(1)
                .and_then(|s| std::str::from_utf8(s).ok())
                .and_then(|s| s.parse::<u32>().ok());
            return Err(if code.is_some_and(|c| c & 0xffff == 99) {
                "PIN entry cancelled"
            } else {
                "PIN entry failed"
            }
            .into());
        }
        if line.starts_with(b"D ") && collect {
            data.extend(decode(&line[2..])?);
            if data.len() > 1024 || data.iter().any(|b| matches!(b, 0 | b'\r' | b'\n')) {
                return Err("Invalid PIN data".into());
            }
        } else if !line.starts_with(b"#") && !line.starts_with(b"S ") {
            return Err("Unexpected pinentry response".into());
        }
    }
}

fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            let pair = input.get(i + 1..i + 3).ok_or("Invalid pinentry escape")?;
            let hex = std::str::from_utf8(pair).map_err(|_| "Invalid pinentry escape")?;
            result.push(u8::from_str_radix(hex, 16).map_err(|_| "Invalid pinentry escape")?);
            i += 3;
        } else {
            result.push(input[i]);
            i += 1;
        }
    }
    Ok(result)
}

fn tty_pin() -> Result<String, String> {
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| "Explicit tty PIN entry requires a controlling terminal")?;
    let settings = Command::new("stty")
        .arg("-g")
        .stdin(tty.try_clone().map_err(|_| "Cannot open terminal")?)
        .output()
        .map_err(|_| "Cannot configure terminal PIN entry")?;
    if !settings.status.success() {
        return Err("Cannot configure terminal PIN entry".into());
    }
    let restore = String::from_utf8(settings.stdout).map_err(|_| "Invalid terminal settings")?;
    let status = Command::new("stty")
        .arg("-echo")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(tty.try_clone().map_err(|_| "Cannot open terminal")?)
        .status()
        .map_err(|_| "Cannot hide terminal PIN entry")?;
    if !status.success() {
        return Err("Cannot hide terminal PIN entry".into());
    }
    let result = (|| {
        write!(tty, "YubiWallet PIN: ")
            .and_then(|_| tty.flush())
            .map_err(|_| "Cannot write terminal prompt")?;
        let mut pin = String::new();
        BufReader::new(tty.try_clone().map_err(|_| "Cannot open terminal")?)
            .read_line(&mut pin)
            .map_err(|_| "Cannot read terminal PIN")?;
        nonempty(pin.trim_end_matches(['\r', '\n']).to_string())
    })();
    let _ = Command::new("stty")
        .arg(restore.trim())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(tty.try_clone().map_err(|_| "Cannot open terminal")?)
        .status();
    let _ = writeln!(tty);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn assuan_data_and_percent_decoding() {
        assert_eq!(
            response(
                &mut Cursor::new(b"# comment\nS status\nD 12%25%20\nD 34\nOK\n"),
                true
            )
            .unwrap(),
            "12% 34"
        );
    }
    #[test]
    fn cancellation_and_error_do_not_expose_child_text() {
        assert_eq!(
            response(&mut Cursor::new(b"ERR 83886179 sensitive\n"), true).unwrap_err(),
            "PIN entry cancelled"
        );
        assert_eq!(
            response(&mut Cursor::new(b"ERR 1 sensitive\n"), true).unwrap_err(),
            "PIN entry failed"
        );
    }
    #[test]
    fn empty_malformed_or_control_data_is_rejected() {
        assert!(nonempty(String::new()).is_err());
        for data in [
            b"D %ZZ\nOK\n".as_slice(),
            b"D %0A\nOK\n",
            b"D %00\nOK\n",
            b"D %\nOK\n",
            b"unexpected\n",
            b"",
        ] {
            assert!(response(&mut Cursor::new(data), true).is_err());
        }
    }
    #[test]
    fn fake_pinentry_subprocess_exchanges_assuan_stdio() {
        for (i, (reply, expected)) in [
            ("'D 12%2534' 'OK'", Ok("12%34")),
            ("'OK'", Err("PIN entry was empty")),
            ("'ERR 83886179 sensitive'", Err("PIN entry cancelled")),
            ("'ERR 1 sensitive'", Err("PIN entry failed")),
            ("'D %ZZ' 'OK'", Err("Invalid pinentry escape")),
        ]
        .into_iter()
        .enumerate()
        {
            let path = std::env::temp_dir().join(format!(
                "yubiwallet-pinentry-test-{}-{i}.sh",
                std::process::id()
            ));
            let script = format!(
                "printf 'OK ready\\n'\nprintf 'DO_NOT_LEAK\\n' >&2\nwhile IFS= read -r cmd; do\ncase \"$cmd\" in\nGETPIN) printf '%s\\n' {reply};;\nBYE) exit 0;;\nSETTITLE\\ *|SETDESC\\ *|SETPROMPT\\ *) printf 'OK\\n';;\n*) exit 1;;\nesac\ndone\n"
            );
            std::fs::write(&path, script).unwrap();
            let result = prompt(Command::new("sh").arg(&path));
            std::fs::remove_file(&path).unwrap();
            assert_eq!(result.as_deref().map_err(String::as_str), expected);
        }
    }
}
