use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub struct StatusIndicator {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    message_sender: mpsc::Sender<String>,
}

impl StatusIndicator {
    pub fn new(message: &str) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let initial_message = message.to_string();

        let (message_sender, message_receiver) = mpsc::channel();

        let handle = thread::spawn(move || {
            let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0;
            let mut current_message = initial_message;

            while running_clone.load(Ordering::Relaxed) {
                // Check for new messages
                if let Ok(new_message) = message_receiver.try_recv() {
                    current_message = new_message;
                }

                print!("\r{} {}", spinner_chars[i % spinner_chars.len()], current_message);
                if let Err(_) = io::stdout().flush() {
                    // If stdout is closed, break out of the loop
                    break;
                }
                i += 1;
                thread::sleep(Duration::from_millis(100));
            }

            // Clear the spinner line
            print!("\r{}\r", " ".repeat(current_message.len() + 2));
            let _ = io::stdout().flush();
        });

        StatusIndicator {
            running,
            handle: Some(handle),
            message_sender,
        }
    }

    pub fn update_message(&self, message: &str) {
        // Send new message to spinner thread
        let _ = self.message_sender.send(message.to_string());
    }

    pub fn finish(&mut self, final_message: Option<&str>) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(handle) = self.handle.take() {
            if let Err(_) = handle.join() {
                // Handle join error gracefully - spinner thread might have panicked
                eprintln!("Warning: Status indicator thread did not shut down cleanly");
            }
        }

        if let Some(msg) = final_message {
            println!("{msg}");
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
