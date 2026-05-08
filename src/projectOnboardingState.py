from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
import os

class SessionState(Enum):
    ACTIVE = "active"
    PLAN_MODE = "plan_mode"

class ContextManager:
    def __init__(self):
        self.state = SessionState.ACTIVE

    def transition_to_plan_mode(self, reason: str):
        self.state = SessionState.PLAN_MODE
        self.write_plan_stub(reason)
        return "Transitioned to Plan Mode. You may only use read-only tools (read_file, grep_search) until the plan is approved by the user."

    def write_plan_stub(self, reason: str):
        os.makedirs(".kla/sessions", exist_ok=True)
        with open(".kla/sessions/PLAN.md", "w") as f:
            f.write(f"# Plan Stub\n\n**Reason for Plan Mode:** {reason}\n\n*Draft your strategy here...*")

@dataclass
class ProjectOnboardingState:
    has_readme: bool
    has_tests: bool
    python_first: bool = True

