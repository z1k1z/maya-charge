use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;
const REPORT: u8 = 8;
const HANDSHAKE: u8 = 1;
const ONLINE: u8 = 3;
const BATTERY: u8 = 4;

#[derive(Debug, PartialEq)]
struct Battery {
    percent: u8,
    charging: bool,
    millivolts: u16,
}

fn packet(command: u8, nonce: Option<[u8; 4]>) -> [u8; 17] {
    let mut bytes = [0u8; 17];
    bytes[0] = REPORT;
    bytes[1] = command;
    if let Some(nonce) = nonce {
        bytes[5] = 8;
        bytes[6..10].copy_from_slice(&nonce);
    }
    bytes[16] = 0x55u8.wrapping_sub(bytes[..16].iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    bytes
}

fn battery_reply(bytes: &[u8]) -> Result<Battery> {
    if bytes.len() != 17 || bytes[0] != REPORT || bytes[1] != BATTERY {
        return Err("Некорректный ответ батареи".into());
    }
    if bytes[6] > 100 || bytes[7] > 1 {
        return Err("Мышь вернула недопустимый уровень или статус заряда".into());
    }
    Ok(Battery {
        percent: bytes[6],
        charging: bytes[7] == 1,
        millivolts: u16::from_be_bytes([bytes[8], bytes[9]]),
    })
}

// Match the vendor output collection observed on this receiver, not the mouse
// motion/keyboard endpoints. hidraw writes keep the kernel drivers attached.
fn supported(uevent: &str, descriptor: &[u8]) -> bool {
    supported_id(uevent)
        && descriptor.windows(3).any(|w| w == [0x06, 0x02, 0xff])
        && descriptor.windows(2).any(|w| w == [0x85, REPORT])
        && descriptor.windows(2).any(|w| w == [0x95, 0x10])
        && descriptor.windows(2).any(|w| w == [0x91, 0x00])
}

fn supported_id(uevent: &str) -> bool {
    let id = uevent.lines().find_map(|s| s.strip_prefix("HID_ID="));
    matches!(
        id,
        Some("0003:00003554:0000F50D" | "0003:00003554:0000F50F")
    )
}

fn discover(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let base = entry.path().join("device");
        let uevent = fs::read_to_string(base.join("uevent")).unwrap_or_default();
        // Most HID devices are unrelated. Do not open their descriptors.
        if !supported_id(&uevent) {
            continue;
        }
        let descriptor = fs::read(base.join("report_descriptor")).unwrap_or_default();
        if supported(&uevent, &descriptor) {
            found.push(Path::new("/dev").join(entry.file_name()));
        }
    }
    found.sort();
    Ok(found)
}

trait Transport {
    fn exchange(&mut self, request: [u8; 17]) -> Result<Option<Vec<u8>>>;
}

struct Hid(File);
impl Transport for Hid {
    fn exchange(&mut self, request: [u8; 17]) -> Result<Option<Vec<u8>>> {
        let written = self
            .0
            .write(&request)
            .map_err(|e| format!("USB write: {e}"))?;
        if written != request.len() {
            return Err("Неполная запись USB-запроса".into());
        }
        read_response(&mut self.0, request[1])
    }
}

fn read_response(file: &mut File, command: u8) -> Result<Option<Vec<u8>>> {
    let deadline = Instant::now() + Duration::from_millis(1000);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        let mut pollfd = libc::pollfd {
            fd: file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one initialized pollfd, valid fd owned by self, bounded timeout.
        let ready = unsafe { libc::poll(&mut pollfd, 1, remaining.as_millis().max(1) as i32) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.to_string());
        }
        if ready == 0 {
            return Ok(None);
        }
        if pollfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err("USB-устройство отключено".into());
        }
        let mut bytes = [0u8; 64];
        match file.read(&mut bytes) {
            Ok(0) => return Err("USB-устройство закрыло соединение".into()),
            Ok(n) if n >= 2 && bytes[0] == REPORT && bytes[1] == command => {
                if n != 17 {
                    return Err(format!("Неверная длина ответа: {n}"));
                }
                return Ok(Some(bytes[..n].to_vec()));
            }
            Ok(_) => continue, // Other HID reports are unrelated to our query.
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn query(transport: &mut impl Transport, nonce: [u8; 4]) -> Result<Option<Battery>> {
    // Some firmware does not acknowledge the handshake; still check online.
    transport.exchange(packet(HANDSHAKE, Some(nonce)))?;
    let online = transport
        .exchange(packet(ONLINE, None))?
        .ok_or("Ресивер не ответил. Разбуди мышь и повтори запрос")?;
    if online.len() != 17 || online[0] != REPORT || online[1] != ONLINE || online[6] > 1 {
        return Err("Некорректный ответ доступности мыши".into());
    }
    if online[6] == 0 {
        return Ok(None);
    }
    let reply = transport
        .exchange(packet(BATTERY, None))?
        .ok_or("Мышь не ответила на запрос заряда")?;
    battery_reply(&reply).map(Some)
}

fn read_device(path: &Path) -> Result<Option<Battery>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| {
            format!(
                "{}: {e}. Проверь udev-права и доступ к /dev/hidraw",
                path.display()
            )
        })?;
    // SAFETY: valid fd; lock is released automatically when the file closes.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Мышь уже опрашивается другим экземпляром mouse-charge".into());
    }
    let mut nonce = [0u8; 4];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut nonce))
        .map_err(|e| e.to_string())?;
    query(&mut Hid(file), nonce)
}

fn output(status: &str, battery: Option<&Battery>, detail: &str) -> Value {
    let text = battery
        .map(|b| format!("{}%{}", b.percent, if b.charging { " ⚡" } else { "" }))
        .unwrap_or_else(|| "—".into());
    let color = match battery {
        Some(b) if b.charging => "primary",
        Some(b) if b.percent <= 20 => "error",
        Some(_) => "secondary",
        None => "on_surface_variant",
    };
    let tooltip = battery
        .map(|b| {
            format!(
                "LAMZU Maya: {}%\n{}\nНапряжение: {} мВ\nНажми для обновления",
                b.percent,
                if b.charging {
                    "Заряжается"
                } else {
                    "Работает от батареи"
                },
                b.millivolts
            )
        })
        .unwrap_or_else(|| format!("LAMZU Maya\n{detail}\nНажми для обновления"));
    json!({ "status": status, "text": text, "glyph": if status == "offline" { "mouse-off" } else { "mouse" },
        "color": color, "tooltip": tooltip, "percent": battery.map(|b| b.percent),
        "charging": battery.map(|b| b.charging), "millivolts": battery.map(|b| b.millivolts) })
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!(
            "mouse-charge [--json | --diagnose]\nЧтение батареи LAMZU Maya (3554:f50d/f50f). Без аргументов — текст."
        );
        return;
    }
    if !args.is_empty() && args != ["--json"] && args != ["--diagnose"] {
        eprintln!("Неизвестные аргументы. Используй --help");
        std::process::exit(2);
    }
    let result = discover(Path::new("/sys/class/hidraw")).and_then(|devices| {
        if args == ["--diagnose"] {
            eprintln!("Подходящие HID-интерфейсы: {devices:?}");
        }
        if devices.is_empty() {
            return Err("Ресивер LAMZU 3554:f50d/f50f не найден".into());
        }
        let mut errors = Vec::new();
        let mut offline = false;
        for device in devices {
            match read_device(&device) {
                Ok(Some(b)) => return Ok(Some(b)),
                Ok(None) => offline = true,
                Err(e) => errors.push(e),
            }
        }
        if offline {
            Ok(None)
        } else {
            Err(errors.join("\n"))
        }
    });
    let value = match &result {
        Ok(Some(b)) => output("ok", Some(b), ""),
        Ok(None) => output(
            "offline",
            None,
            "Мышь спит, выключена или вне зоны действия",
        ),
        Err(e) => output("error", None, e),
    };
    if args == ["--json"] {
        println!("{value}");
    } else {
        println!("{}", value["tooltip"].as_str().unwrap());
    }
    if result.is_err() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn packet_pipe() -> (File, File) {
        use std::os::fd::FromRawFd;
        let mut fds = [-1; 2];
        // SAFETY: writable array of two descriptors. Packet mode preserves
        // report boundaries like hidraw, without needing USB or sockets.
        assert_eq!(
            unsafe {
                libc::pipe2(
                    fds.as_mut_ptr(),
                    libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_DIRECT,
                )
            },
            0
        );
        // SAFETY: pipe2 succeeded; each fd is newly owned exactly once.
        unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) }
    }

    #[test]
    fn real_poll_ignores_unrelated_reports_and_reads_battery() {
        let (mut input, mut peer) = packet_pipe();
        peer.write_all(&[2, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        peer.write_all(&reply(BATTERY, 72)).unwrap();
        let response = read_response(&mut input, BATTERY).unwrap().unwrap();
        assert_eq!(battery_reply(&response).unwrap().percent, 72);
    }

    #[test]
    fn real_poll_timeout_sleeps_without_busy_loop() {
        fn cpu_seconds() -> f64 {
            let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
            // SAFETY: valid writable rusage; only read after a successful call.
            assert_eq!(
                unsafe { libc::getrusage(libc::RUSAGE_THREAD, usage.as_mut_ptr()) },
                0
            );
            let usage = unsafe { usage.assume_init() };
            (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as f64
                + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1_000_000.0
        }
        let (mut input, _peer) = packet_pipe();
        let start = Instant::now();
        let cpu = cpu_seconds();
        assert!(read_response(&mut input, BATTERY).unwrap().is_none());
        let used = cpu_seconds() - cpu;
        assert!(start.elapsed() >= Duration::from_millis(900));
        // Wide tolerance for instrumented builds, but a spinning wait fails.
        assert!(used < 0.15, "Idle USB timeout consumed {used} CPU seconds");
    }

    #[test]
    fn real_poll_rejects_truncated_matching_report() {
        let (mut input, mut peer) = packet_pipe();
        peer.write_all(&[REPORT, BATTERY, 0]).unwrap();
        assert!(read_response(&mut input, BATTERY).is_err());
    }
    struct Fake {
        replies: VecDeque<Option<Vec<u8>>>,
        sent: Vec<u8>,
    }
    impl Transport for Fake {
        fn exchange(&mut self, request: [u8; 17]) -> Result<Option<Vec<u8>>> {
            self.sent.push(request[1]);
            Ok(self.replies.pop_front().unwrap())
        }
    }
    fn reply(command: u8, value: u8) -> Vec<u8> {
        let mut p = packet(command, None);
        p[6] = value;
        p.to_vec()
    }
    #[test]
    fn request_checksum_and_nonce() {
        assert_eq!(
            packet(BATTERY, None),
            [8, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x49]
        );
        let p = packet(HANDSHAKE, Some([0xff, 2, 3, 4]));
        assert_eq!(&p[5..10], &[8, 0xff, 2, 3, 4]);
        assert_eq!(p.iter().fold(0u8, |a, b| a.wrapping_add(*b)), 0x55);
    }
    #[test]
    fn complete_exchange_and_charge_parsing() {
        let mut b = reply(BATTERY, 87);
        b[7] = 1;
        b[8..10].copy_from_slice(&4100u16.to_be_bytes());
        let mut fake = Fake {
            replies: [None, Some(reply(ONLINE, 1)), Some(b)].into(),
            sent: vec![],
        };
        assert_eq!(
            query(&mut fake, [1; 4]).unwrap(),
            Some(Battery {
                percent: 87,
                charging: true,
                millivolts: 4100
            })
        );
        assert_eq!(fake.sent, [HANDSHAKE, ONLINE, BATTERY]);
    }
    #[test]
    fn sleeping_mouse_skips_battery_query() {
        let mut fake = Fake {
            replies: [None, Some(reply(ONLINE, 0))].into(),
            sent: vec![],
        };
        assert_eq!(query(&mut fake, [1; 4]).unwrap(), None);
        assert_eq!(fake.sent, [HANDSHAKE, ONLINE]);
    }
    #[test]
    fn missing_online_is_error_not_zero() {
        let mut fake = Fake {
            replies: [None, None].into(),
            sent: vec![],
        };
        assert!(query(&mut fake, [1; 4]).is_err());
        let value = output("error", None, "Нет доступа");
        assert!(value["percent"].is_null());
        assert_eq!(value["text"], "—");
    }
    #[test]
    fn reject_malformed_battery_and_accept_real_zero() {
        assert!(battery_reply(&[0; 17]).is_err());
        assert!(battery_reply(&reply(BATTERY, 101)).is_err());
        assert!(battery_reply(&reply(BATTERY, 50)[..10]).is_err());
        assert_eq!(battery_reply(&reply(BATTERY, 0)).unwrap().percent, 0);
        let mut bad = reply(BATTERY, 50);
        bad[7] = 255;
        assert!(battery_reply(&bad).is_err());
    }
    #[test]
    fn discovery_rejects_other_hardware_and_motion_interface() {
        let descriptor = [0x06, 0x02, 0xff, 0x85, 8, 0x95, 0x10, 0x91, 0];
        assert!(supported("HID_ID=0003:00003554:0000F50D\n", &descriptor));
        assert!(!supported("HID_ID=0003:000036B0:0000352F\n", &descriptor));
        assert!(!supported("HID_ID=0003:00003554:0000F50D\n", &[5, 1, 9, 2]));
    }
}
