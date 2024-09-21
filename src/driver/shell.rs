use alloc::collections::vec_deque::VecDeque;
use alloc::vec::Vec;
use bstr::{ByteSlice, Split};
use core::iter;
use embassy_executor::task;
use embedded_io_async::{Read, Write};
use esp_hal::peripherals::USB_DEVICE;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Async;

use crate::log_init;

static COMMAND_INFO: &[(&[u8], CommandInfo)] = &[
    (b"help", commands::HELP_INFO),
    (b"echo_enable", commands::ECHO_ENABLE_INFO),
    (b"format", commands::FORMAT_INFO),
];

const PROMPT: &str = "xenon> ";

macro_rules! bytes {
    (
        $($slice:expr),* $(,)?
    ) => {
        // Creating a reference here may reduce code monomorphization costs for different sized
        // arrays, since it's intended to be used by functions which take an iterator of &[u8] (or
        // &&[u8]).
        &[
            $(
                bstr::B($slice)
            ),*
        ] as &[&[u8]]
    }
}

#[task]
pub async fn start(usb: USB_DEVICE) {
    let mut shell = Shell::new(usb);

    log_init("shell");

    loop {
        shell.echo(PROMPT).await;
        shell.recv().await;

        let bytes = iter::once(b'\n')
            .chain(shell.buffer.drain(..))
            .collect::<Vec<_>>();
        let bytes = bytes.trim();

        shell.send("\n").await;
        let mut split = ByteSlice::split_str(bytes, " ");

        shell.handle_command(&mut split).await;
    }
}

pub struct Shell {
    serial: UsbSerialJtag<'static, Async>,
    buffer: VecDeque<u8>,
    echo: bool,
}

// NOTE: .unwrap() can be done on the Read/Write methods for the USB serial device because the Err
// variant is Infallible.
impl Shell {
    pub fn new(usb: USB_DEVICE) -> Self {
        Self {
            serial: UsbSerialJtag::new_async(usb),
            buffer: VecDeque::new(),
            echo: true,
        }
    }

    pub async fn send(&mut self, data: impl AsRef<[u8]>) {
        let data = data.as_ref();

        self.serial.write_all(data).await.unwrap();
    }

    pub async fn send_iter<T: AsRef<[u8]>>(&mut self, data: impl IntoIterator<Item = T>) {
        for slice in data.into_iter() {
            self.send(slice).await;
        }
    }

    pub async fn recv(&mut self) {
        let mut buf = [0u8; 64];

        loop {
            let bytes_read = self.serial.read(&mut buf).await.unwrap();

            let slice = &buf[..bytes_read];

            for &byte in slice.iter() {
                match byte {
                    b'\r' | b'\n' => return,
                    b'\x08' => {
                        if self.buffer.pop_back().is_some() {
                            // remove the previous character, and replace it with a space in the serial
                            // console, then go back one character.
                            self.echo(b"\x08\x20\x08").await;
                        }
                    }
                    b => {
                        self.buffer.push_back(b);
                        self.echo_one(b).await;
                    }
                }
            }

            self.flush().await;
        }
    }

    pub async fn wait_for_input(&mut self) {
        let mut buf = [0u8; 1];
        self.serial.read(&mut buf).await.unwrap();
    }

    pub async fn flush(&mut self) {
        self.serial.flush().await.unwrap();
    }

    pub async fn echo_one(&mut self, byte: u8) {
        self.echo(&[byte]).await
    }

    pub async fn echo(&mut self, data: impl AsRef<[u8]>) {
        if self.echo {
            self.send(data).await
        }
    }

    async fn handle_command(&mut self, args: &mut Split<'_, '_>) {
        let Some(command) = args.next() else {
            return;
        };

        match command {
            b"help" => commands::help(self, args).await,
            b"echo_enable" => commands::echo_enable(self, args).await,
            b"format" => commands::format(self, args).await,
            unknown => commands::unknown_command(self, unknown).await,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct CommandInfo {
    help: &'static str,
}

mod commands {
    use crate::fs::FILESYSTEM;

    use super::CommandInfo;
    use super::Shell;
    use bstr::Split;
    use embassy_futures::select::{select, Either};
    use embassy_time::Timer;
    use paste::paste;

    macro_rules! command {
        (
            #[help(
                $($help_str:literal),* $(,)?
            )]
            $vis:vis async fn $name:ident ( $shell:pat, $args:pat ) $code:block
        ) => {
            paste! {
                pub static [<$name:upper _INFO>]: CommandInfo = CommandInfo {
                    help: concat!(
                        $(
                            $help_str,
                            "\n",
                        )*
                    ),
                };
            }

            $vis async fn $name ($shell: &mut Shell, $args: &mut Split<'_, '_>) $code
        }
    }

    command! {
        #[help(
            "displays this help text.",
        )]
        pub async fn help(shell, _) {
            shell.send_iter(bytes![
                "Xenon v", crate::VERSION, " serial shell\n",
                "Authors: Factorial\n\n"
            ]).await;

            for (name, info) in super::COMMAND_INFO.iter() {
                shell.send_iter(bytes![
                    name,
                    ": ",
                    info.help,
                    "\n",
                ]).await;
            }

            shell.flush().await;
        }
    }

    command! {
        #[help(
            "enables or disables input echo",
            "USAGE:",
            "echo_enable true  # Enables input echo",
            "echo_enable false # Disables input echo"
        )]
        pub async fn echo_enable(shell, args) {
            let echo = match args.next() {
                Some(b"true") => Some(true),
                Some(b"false") => Some(false),
                Some(unknown) => {
                    shell.send_iter(bytes![
                        "unknown option \"",
                        unknown,
                        "\", expected either `true` or `false`\n"
                    ]).await;

                    None
                }
                None => {
                    shell.send("missing parameter, expected either `true` or `false`\n").await;
                    None
                }
            };

            if let Some(enable) = echo {
                shell.echo = enable
            }

            shell.flush().await;
        }
    }

    command! {
        #[help(
            "Formats the in-flash filesystem. After running this, all in-flash data will be \
             automatically formatted after 10 seconds of no user input.",
            "WARNING: Formatting the filesystem will cause _IRREVERSIBLE_ data loss and may cause \
             open files to become corrupt. This should only be done if you're sure that you're \
             okay with that."
        )]
        pub async fn format(shell, _) {
            shell.send("Flash will be formatted in 10 seconds; press any key to abort.\n").await;

            let old_echo = shell.echo;
            shell.echo = false;

            let selected = select(
                shell.wait_for_input(),
                Timer::after_secs(10),
            ).await;

            shell.echo = old_echo;

            match selected {
                Either::First(_) => {
                    shell.send("Flash format aborted.\n").await;
                }
                Either::Second(_) => {
                    shell.send("Formatting flash. This might take a bit.\n").await;

                    match FILESYSTEM.format().await {
                        Ok(_) => shell.send("Format complete.\n").await,
                        Err(e) => shell.send(alloc::format!("Format failed! Error: {e}")).await,
                    }
                }
            }
        }
    }

    pub async fn unknown_command(shell: &mut Shell, cmd: &[u8]) {
        shell
            .send_iter(bytes!["unknown command: \"", cmd, "\"\n"])
            .await;
        shell.flush().await;
    }
}
