# Agent Data Model Specification

## Enums

### AgentStatus
```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Running,
    Stopped,
    Paused,
}
```

Implement `Display` trait:
- Running -> "RUNNING"
- Stopped -> "STOPPED"
- Paused -> "PAUSED"

## Structs

### Agent
```rust
pub struct Agent {
    pub name: String,
    pub status: AgentStatus,
    pub start_time: Option<Instant>,
    pub output: Vec<String>,
    pub iteration: u32,
}
```

### Agent Methods
```rust
impl Agent {
    pub fn new(name: &str) -> Self
    pub fn uptime_secs(&self) -> u64
    pub fn add_output(&mut self, line: &str)
    pub fn start(&mut self)
    pub fn stop(&mut self)
    pub fn pause(&mut self)
    pub fn resume(&mut self)
}
```

## Mock Data

### Mock Output Lines

Alpha agent (AI research theme):
```
[HH:MM:SS] Starting analysis loop...
[HH:MM:SS] Loading market data from cache
[HH:MM:SS] Analyzing RSI divergence patterns
[HH:MM:SS] Found 3 potential signals
[HH:MM:SS] Backtesting strategy_v12...
[HH:MM:SS] Results: Sharpe 2.1, MaxDD -12%, Win 64%
[HH:MM:SS] Generating improved strategy
[HH:MM:SS] Writing src/strategies/momentum_v13.rs
[HH:MM:SS] Running cargo test...
[HH:MM:SS] All tests passed (23/23)
[HH:MM:SS] Committing changes...
[HH:MM:SS] Iteration complete. Exiting.
```

Gamma agent (data processing theme):
```
[HH:MM:SS] Initializing data pipeline
[HH:MM:SS] Fetching datasets from S3
[HH:MM:SS] Processing batch 1/10
[HH:MM:SS] Applying transformations
[HH:MM:SS] Validating schema integrity
[HH:MM:SS] Writing to parquet: data/processed/batch_001.parquet
[HH:MM:SS] Updating metadata index
[HH:MM:SS] Pipeline complete. 1.2GB processed.
```

### Create Mock Agents Function
```rust
pub fn create_mock_agents() -> Vec<Agent> {
    vec![
        Agent::new("alpha"),   // Running with research output
        Agent::new("beta"),    // Stopped, empty output
        Agent::new("gamma"),   // Running with data processing output
    ]
}
```

Initialize alpha and gamma as Running with some initial output.
Initialize beta as Stopped with empty output.

## Mock Output Generator

For running agents, periodically add new output lines:
- Every 2-5 seconds (randomized)
- Pick from theme-appropriate output pool
- Increment iteration counter every ~10 lines (simulates loop restart)
