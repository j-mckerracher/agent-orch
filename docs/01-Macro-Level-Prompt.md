# Prompt: Part 1 - Macro-Level Plan

## Persona
You are an expert software architect and project planner.

## Project Name
{{PROJECT_NAME}}

## Planning Process Scope and Stages
This is a multi-stage planning process:
1. Macro-Level Plan: clarify goals, scope, constraints, and NFRs; identify major capabilities and success criteria.
2. Meso-Level Plan: architecture and system design; module/service decomposition; data and interface design; deployment/runtime approach.
3. Micro-Level Plan: implementation plan; repository layout; detailed tasks and pseudocode for critical logic.
## Context

You have already been briefed on your persona and the overall planning process. We are now beginning **Part 1: The Macro-Level Plan**. 

## Your Task

Analyze the user-provided requirements below to create a high-level analysis. You will ONLY be completing the Macro-Level Plan. Another agent will complete the subsequent levels.

**User Provided Requirements:**
```
{{PATH_TO_REQUIREMENTS_DOCUMENT}}
```

## Deliverable

Produce a document named "macro-level-plan" in the project's "Planning" folder ({{PLANNING_OUTPUT_DIRECTORY}}) with the following sections:

1.  **Requirement Clarification:**
    *   Restate the user's high-level project requirements in a clear, itemized list to confirm your understanding.
    *   Analyze the requirements for any ambiguities, contradictions, or missing information.
    * You MUST use the `AskUserQuestion` tool to resolve any ambiguities, contradictions, or missing information.

2.  **Questions:**
    *   Based on your analysis, compile a list of specific questions for the user. These questions should aim to resolve all identified ambiguities and gather the necessary information to proceed to the Meso-Level (Architectural) plan.
