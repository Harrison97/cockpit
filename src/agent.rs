//! Agent data model for the God Agent Console
//!
//! This module defines the Agent struct and AgentStatus enum used throughout the TUI.

#![allow(dead_code)] // Items will be used as more features are implemented

use rand::Rng;
use ratatui::style::Color;
use std::fmt;
use std::time::{Duration, Instant};

/// Mock output lines for the Alpha agent (AI research theme)
pub const ALPHA_OUTPUTS: &[&str] = &[
    "Starting analysis loop...",
    "Loading market data from cache",
    "Analyzing RSI divergence patterns",
    "Found 3 potential signals",
    "Backtesting strategy_v12...",
    "Results: Sharpe 2.1, MaxDD -12%, Win 64%",
    "Generating improved strategy",
    "Writing src/strategies/momentum_v13.rs",
    "Running cargo test...",
    "All tests passed (23/23)",
    "Committing changes...",
    "Iteration complete. Exiting.",
];

/// Mock output lines for the Gamma agent (data processing theme)
pub const GAMMA_OUTPUTS: &[&str] = &[
    "Initializing data pipeline",
    "Fetching datasets from S3",
    "Processing batch 1/10",
    "Applying transformations",
    "Validating schema integrity",
    "Writing to parquet: data/processed/batch_001.parquet",
    "Updating metadata index",
    "Pipeline complete. 1.2GB processed.",
];

/// Status of an AI agent
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Running,
    Stopped,
    Paused,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentStatus::Running => write!(f, "RUNNING"),
            AgentStatus::Stopped => write!(f, "STOPPED"),
            AgentStatus::Paused => write!(f, "PAUSED"),
        }
    }
}

impl AgentStatus {
    /// Returns the color to use when displaying this status in the UI
    pub fn color(&self) -> Color {
        match self {
            AgentStatus::Running => Color::Green,
            AgentStatus::Stopped => Color::Red,
            AgentStatus::Paused => Color::Yellow,
        }
    }
}

/// Represents an AI agent being monitored by the console
pub struct Agent {
    pub name: String,
    pub status: AgentStatus,
    pub start_time: Option<Instant>,
    pub output: Vec<String>,
    pub iteration: u32,
    /// Index tracking which mock output line to add next
    pub mock_output_index: usize,
    /// Count of output lines since last iteration increment
    pub lines_since_iteration: u32,
    /// When to next add mock output (for randomized intervals)
    pub next_output_at: Instant,
}

impl Agent {
    /// Creates a new agent with the given name
    ///
    /// The agent starts in Stopped status with no output.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: AgentStatus::Stopped,
            start_time: None,
            output: Vec::new(),
            iteration: 0,
            mock_output_index: 0,
            lines_since_iteration: 0,
            next_output_at: Instant::now(),
        }
    }

    /// Generates a random delay between 2-5 seconds
    fn random_output_delay() -> Duration {
        let mut rng = rand::thread_rng();
        Duration::from_millis(rng.gen_range(2000..=5000))
    }

    /// Schedules the next output at a random time 2-5 seconds from now
    pub fn schedule_next_output(&mut self) {
        self.next_output_at = Instant::now() + Self::random_output_delay();
    }

    /// Returns true if it's time to add the next output line
    pub fn is_output_due(&self) -> bool {
        self.status == AgentStatus::Running && Instant::now() >= self.next_output_at
    }

    /// Returns the uptime in seconds, or 0 if the agent has no start time
    pub fn uptime_secs(&self) -> u64 {
        match self.start_time {
            Some(start) => start.elapsed().as_secs(),
            None => 0,
        }
    }

    /// Adds an output line to the agent's output buffer
    pub fn add_output(&mut self, line: &str) {
        self.output.push(line.to_string());
    }

    /// Starts the agent, setting status to Running and recording start time
    pub fn start(&mut self) {
        self.status = AgentStatus::Running;
        self.start_time = Some(Instant::now());
        self.schedule_next_output();
    }

    /// Stops the agent, setting status to Stopped and clearing start time
    pub fn stop(&mut self) {
        self.status = AgentStatus::Stopped;
        self.start_time = None;
    }

    /// Pauses the agent, setting status to Paused (keeps start time for uptime tracking)
    pub fn pause(&mut self) {
        self.status = AgentStatus::Paused;
    }

    /// Resumes a paused agent, setting status back to Running
    pub fn resume(&mut self) {
        if self.status == AgentStatus::Paused {
            self.status = AgentStatus::Running;
        }
    }

    /// Returns the mock output lines for this agent based on its name
    fn mock_outputs(&self) -> &'static [&'static str] {
        match self.name.as_str() {
            "alpha" => ALPHA_OUTPUTS,
            "gamma" => GAMMA_OUTPUTS,
            // Default to alpha outputs for unknown agents
            _ => ALPHA_OUTPUTS,
        }
    }

    /// Adds the next mock output line, cycling through the output list
    ///
    /// Increments the iteration counter every ~10 lines.
    /// Schedules the next output at a random interval 2-5 seconds from now.
    pub fn add_next_mock_output(&mut self) {
        let outputs = self.mock_outputs();
        if outputs.is_empty() {
            return;
        }

        // Get the next output line (cycling)
        let line = outputs[self.mock_output_index % outputs.len()];
        self.add_output(line);
        self.mock_output_index += 1;

        // Track lines and increment iteration every ~10 lines
        self.lines_since_iteration += 1;
        if self.lines_since_iteration >= 10 {
            self.iteration += 1;
            self.lines_since_iteration = 0;
        }

        // Schedule the next output
        self.schedule_next_output();
    }
}

/// Creates mock agents for demonstration purposes
///
/// Returns 3 agents:
/// - alpha: Running with initial research-themed output
/// - beta: Stopped with empty output
/// - gamma: Running with data processing output
pub fn create_mock_agents() -> Vec<Agent> {
    let mut alpha = Agent::new("alpha");
    alpha.start();
    alpha.iteration = 1;
    // Add initial output lines for alpha
    for line in ALPHA_OUTPUTS.iter().take(4) {
        alpha.add_output(line);
    }
    alpha.mock_output_index = 4; // Next line to add
    alpha.lines_since_iteration = 4; // Track initial lines

    let beta = Agent::new("beta");
    // beta stays Stopped with no output

    let mut gamma = Agent::new("gamma");
    gamma.start();
    gamma.iteration = 1;
    // Add initial output lines for gamma
    for line in GAMMA_OUTPUTS.iter().take(3) {
        gamma.add_output(line);
    }
    gamma.mock_output_index = 3; // Next line to add
    gamma.lines_since_iteration = 3; // Track initial lines

    vec![alpha, beta, gamma]
}
