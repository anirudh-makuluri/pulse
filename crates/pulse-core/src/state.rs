use crate::error::{PulseError, Result};
use crate::models::TaskStatus;

/// Returns whether a transition from `from` to `to` is allowed.
///
/// | From \ To | Inbox | Today | Next | Waiting | Done |
/// | Inbox     |  —    |  ✓    |  ✓   |    ✓    |  ✓   |
/// | Today     |  ✓    |  —    |  ✓   |    ✓    |  ✓   |
/// | Next      |  ✓    |  ✓    |  —   |    ✓    |  ✓   |
/// | Waiting   |  ✓    |  ✓    |  ✓   |    —    |  ✓   |
/// | Done      |  ✓*   |  ✗    |  ✗   |    ✗    |  —   |
///
/// * Done may only reopen to Inbox.
pub fn can_transition(from: TaskStatus, to: TaskStatus) -> bool {
    if from == to {
        return false;
    }
    match from {
        TaskStatus::Done => to == TaskStatus::Inbox,
        TaskStatus::Inbox | TaskStatus::Today | TaskStatus::Next | TaskStatus::Waiting => true,
    }
}

/// Validate a transition or return `InvalidTransition`.
pub fn validate_transition(from: TaskStatus, to: TaskStatus) -> Result<()> {
    if can_transition(from, to) {
        Ok(())
    } else {
        Err(PulseError::InvalidTransition {
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TaskStatus::*;

    #[test]
    fn open_states_can_move_anywhere_else() {
        let opens = [Inbox, Today, Next, Waiting];
        for from in opens {
            for to in [Inbox, Today, Next, Waiting, Done] {
                if from == to {
                    assert!(!can_transition(from, to), "{from} -> {to}");
                } else {
                    assert!(can_transition(from, to), "{from} -> {to}");
                }
            }
        }
    }

    #[test]
    fn done_only_reopens_to_inbox() {
        assert!(can_transition(Done, Inbox));
        assert!(!can_transition(Done, Today));
        assert!(!can_transition(Done, Next));
        assert!(!can_transition(Done, Waiting));
        assert!(!can_transition(Done, Done));
    }

    #[test]
    fn validate_errors_on_invalid() {
        assert!(validate_transition(Done, Today).is_err());
        assert!(validate_transition(Inbox, Today).is_ok());
    }
}
