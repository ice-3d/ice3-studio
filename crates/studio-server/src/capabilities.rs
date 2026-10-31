use studio_core::agent_contract::AgentAccess;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    ReviewRequired,
    Autonomous,
}

impl WriteMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "review-required" => Ok(Self::ReviewRequired),
            "autonomous" => Ok(Self::Autonomous),
            _ => Err(format!("unsupported write mode: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Propose,
    Approve,
    AutonomousWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAuthority {
    ReadOnly,
    AgentProposal,
    HumanApproval,
    AutonomousAgent,
}

pub fn classify_request_authority(
    mode: WriteMode,
    agent_token: Option<&str>,
    approval_cookie: Option<&str>,
    expected_agent: &str,
    expected_approval: &str,
) -> RequestAuthority {
    let agent_valid = agent_token
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected_agent.as_bytes()));
    let approval_valid = approval_cookie
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected_approval.as_bytes()));
    match mode {
        WriteMode::ReviewRequired if approval_valid => RequestAuthority::HumanApproval,
        WriteMode::ReviewRequired if agent_valid => RequestAuthority::AgentProposal,
        WriteMode::Autonomous if agent_valid => RequestAuthority::AutonomousAgent,
        _ => RequestAuthority::ReadOnly,
    }
}

pub fn agent_access(authority: RequestAuthority) -> AgentAccess {
    match authority {
        RequestAuthority::ReadOnly => AgentAccess {
            can_read: true,
            can_propose: false,
            can_commit: false,
            can_apply_directly: false,
            can_discard: false,
        },
        RequestAuthority::AgentProposal => AgentAccess {
            can_read: true,
            can_propose: true,
            can_commit: false,
            can_apply_directly: false,
            can_discard: false,
        },
        RequestAuthority::HumanApproval | RequestAuthority::AutonomousAgent => AgentAccess {
            can_read: true,
            can_propose: true,
            can_commit: true,
            can_apply_directly: true,
            can_discard: true,
        },
    }
}

pub fn authorize_write(
    mode: WriteMode,
    method: &str,
    agent_token: Option<&str>,
    approval_cookie: Option<&str>,
    expected_agent: &str,
    expected_approval: &str,
) -> Option<Capability> {
    let proposal_method = matches!(
        method,
        "changeset.create"
            | "changeset.addOperations"
            | "changeset.removeOperations"
            | "changeset.evaluate"
            | "changeset.getCandidate"
            | "export.sourceGlb"
            | "export.runtimePreview"
    );
    let approval_method = matches!(
        method,
        "project.create"
            | "project.open"
            | "project.save"
            | "project.close"
            | "document.apply"
            | "document.undo"
            | "document.redo"
            | "changeset.commit"
            | "changeset.discard"
    );
    let agent_valid = agent_token
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected_agent.as_bytes()));
    let approval_valid = approval_cookie
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected_approval.as_bytes()));

    match mode {
        WriteMode::ReviewRequired if proposal_method && agent_valid => Some(Capability::Propose),
        WriteMode::ReviewRequired if (proposal_method || approval_method) && approval_valid => {
            Some(Capability::Approve)
        }
        WriteMode::Autonomous if agent_valid => Some(Capability::AutonomousWrite),
        _ => None,
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
