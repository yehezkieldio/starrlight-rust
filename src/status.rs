use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub struct StatusIndicator {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl StatusIndicator {
    pub fn new(message: &str) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let message = message.to_string();

        let handle = thread::spawn(move || {
            let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0;

            while running_clone.load(Ordering::Relaxed) {
                print!("\r{} {}", spinner_chars[i % spinner_chars.len()], message);
                io::stdout().flush().unwrap();
                i += 1;
                thread::sleep(Duration::from_millis(100));
            }

            // Clear the spinner line
            print!("\r{}\r", " ".repeat(message.len() + 2));
            io::stdout().flush().unwrap();
        });

        StatusIndicator {
            running,
            handle: Some(handle),
        }
    }

    pub fn update_message(&self, message: &str) {
        // For simplicity, we'll just print a new line with the updated message
        // In a more sophisticated implementation, you could use channels to communicate with the spinner thread
        println!("\n{}", message);
    }

    pub fn finish(&mut self, final_message: Option<&str>) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }

        if let Some(msg) = final_message {
            println!("{}", msg);
        }
    }
}

impl Drop for StatusIndicator {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
