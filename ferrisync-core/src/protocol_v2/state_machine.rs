use std::fmt;

/// Events emitted by the sync session state machine.
///
/// The state machine is a pure data structure — it processes input events
/// and produces output events and state transitions. No I/O is performed
/// inside the machine itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEvent {
    /// Handshake completed, session is active.
    HandshakeComplete {
        peer_device_id: String,
        peer_device_name: String,
    },
    /// Ready to exchange file indexes.
    ReadyForIndex,
    /// An index has been received and reconciled.
    IndexReceived {
        folder_id: String,
        upload_count: usize,
        download_count: usize,
        conflict_count: usize,
    },
    /// A file transfer started.
    TransferStarted {
        path: String,
        direction: TransferDirection,
    },
    /// A file transfer completed.
    TransferComplete {
        path: String,
        direction: TransferDirection,
    },
    /// A file transfer failed.
    TransferFailed { path: String, error: String },
    /// Session ended normally.
    SessionComplete {
        pushed: usize,
        pulled: usize,
        conflicts: usize,
    },
    /// Session ended with an error.
    SessionError { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,
    Download,
}

impl fmt::Display for TransferDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upload => write!(f, "upload"),
            Self::Download => write!(f, "download"),
        }
    }
}

/// Current state of a sync session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Waiting for Hello exchange.
    Handshaking,
    /// Hello received, ready for index exchange.
    Active,
    /// Index received, computing sync plan.
    Reconciling,
    /// Executing transfer plan.
    Transferring,
    /// All transfers done, recording history.
    Finalizing,
    /// Session ended.
    Complete,
    /// Session ended with error.
    Error,
}

impl SessionState {
    /// Whether this state is terminal (no further transitions).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Error)
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshaking => write!(f, "handshaking"),
            Self::Active => write!(f, "active"),
            Self::Reconciling => write!(f, "reconciling"),
            Self::Transferring => write!(f, "transferring"),
            Self::Finalizing => write!(f, "finalizing"),
            Self::Complete => write!(f, "complete"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// A simple state machine for tracking sync session lifecycle.
///
/// State transitions:
///
/// ```text
/// Handshaking → Active → Reconciling → Transferring → Finalizing → Complete
///     ↓           ↓          ↓              ↓              ↓
///   Error       Error      Error          Error          Error
/// ```
#[derive(Debug)]
pub struct StateMachine {
    state: SessionState,
    pushed: usize,
    pulled: usize,
    conflicts: usize,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: SessionState::Handshaking,
            pushed: 0,
            pulled: 0,
            conflicts: 0,
        }
    }

    /// Current state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Handshake completed, transition to Active.
    pub fn on_handshake_complete(&mut self) -> Result<(), TransitionError> {
        self.require_state(SessionState::Handshaking)?;
        self.state = SessionState::Active;
        Ok(())
    }

    /// Index received, transition to Reconciling.
    pub fn on_index_received(&mut self) -> Result<(), TransitionError> {
        self.require_state(SessionState::Active)?;
        self.state = SessionState::Reconciling;
        Ok(())
    }

    /// Reconciliation complete, transition to Transferring.
    pub fn on_transfer_start(&mut self) -> Result<(), TransitionError> {
        self.require_state(SessionState::Reconciling)?;
        self.state = SessionState::Transferring;
        Ok(())
    }

    /// Transfer complete, transition to Finalizing.
    pub fn on_transfer_complete(&mut self) -> Result<(), TransitionError> {
        self.require_state(SessionState::Transferring)?;
        self.state = SessionState::Finalizing;
        Ok(())
    }

    /// Record a successful upload.
    pub fn record_upload(&mut self) {
        self.pushed += 1;
    }

    /// Record a successful download.
    pub fn record_download(&mut self) {
        self.pulled += 1;
    }

    /// Record a conflict.
    pub fn record_conflict(&mut self) {
        self.conflicts += 1;
    }

    /// Finalize the session.
    pub fn finalize(&mut self) -> Result<(), TransitionError> {
        self.require_state(SessionState::Finalizing)?;
        self.state = SessionState::Complete;
        Ok(())
    }

    /// Transition to error state from any non-terminal state.
    pub fn error(&mut self) -> Result<(), TransitionError> {
        if self.state.is_terminal() {
            return Err(TransitionError::AlreadyTerminal);
        }
        self.state = SessionState::Error;
        Ok(())
    }

    /// Get final stats.
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            pushed: self.pushed,
            pulled: self.pulled,
            conflicts: self.conflicts,
        }
    }

    fn require_state(&self, expected: SessionState) -> Result<(), TransitionError> {
        if self.state != expected {
            Err(TransitionError::InvalidTransition {
                from: self.state,
                to: expected,
            })
        } else {
            Ok(())
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    InvalidTransition {
        from: SessionState,
        to: SessionState,
    },
    AlreadyTerminal,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "cannot transition from {from} to {to}")
            }
            Self::AlreadyTerminal => write!(f, "session is already terminal"),
        }
    }
}

impl std::error::Error for TransitionError {}

#[derive(Debug, Clone)]
pub struct SessionStats {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions() {
        let mut sm = StateMachine::new();
        assert_eq!(sm.state(), SessionState::Handshaking);

        sm.on_handshake_complete().unwrap();
        assert_eq!(sm.state(), SessionState::Active);

        sm.on_index_received().unwrap();
        assert_eq!(sm.state(), SessionState::Reconciling);

        sm.on_transfer_start().unwrap();
        assert_eq!(sm.state(), SessionState::Transferring);
        sm.record_upload();
        sm.record_download();
        sm.record_conflict();

        sm.on_transfer_complete().unwrap();
        assert_eq!(sm.state(), SessionState::Finalizing);

        sm.finalize().unwrap();
        assert_eq!(sm.state(), SessionState::Complete);
        assert!(sm.state().is_terminal());

        let stats = sm.stats();
        assert_eq!(stats.pushed, 1);
        assert_eq!(stats.pulled, 1);
        assert_eq!(stats.conflicts, 1);
    }

    #[test]
    fn invalid_transition_returns_error() {
        let mut sm = StateMachine::new();
        let err = sm.on_index_received().unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: SessionState::Handshaking,
                to: SessionState::Active,
            }
        ));
    }

    #[test]
    fn error_from_any_non_terminal() {
        let mut sm = StateMachine::new();
        sm.error().unwrap();
        assert_eq!(sm.state(), SessionState::Error);
    }

    #[test]
    fn error_from_terminal_fails() {
        let mut sm = StateMachine::new();
        sm.on_handshake_complete().unwrap();
        sm.on_index_received().unwrap();
        sm.on_transfer_start().unwrap();
        sm.on_transfer_complete().unwrap();
        sm.finalize().unwrap();
        assert_eq!(sm.error(), Err(TransitionError::AlreadyTerminal));
    }

    #[test]
    fn state_display() {
        assert_eq!(SessionState::Handshaking.to_string(), "handshaking");
        assert_eq!(SessionState::Error.to_string(), "error");
    }
}
