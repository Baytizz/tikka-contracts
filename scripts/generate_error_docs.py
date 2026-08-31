#!/usr/bin/env python3
"""
Script to auto-generate error code documentation from Rust Error enums.
This script parses the Error enum in contracts/raffle-instance/src/lib.rs,
the ContractError enum in contracts/raffle-factory/src/lib.rs, and the
ProtocolError enum in contracts/raffle-shared/src/errors.rs, then generates
the markdown table for docs/ERRORS.md.
"""

import re
import sys
from pathlib import Path


def parse_error_enum(file_path, enum_name):
    """Parse an error enum from the Rust source file."""
    with open(file_path, 'r') as f:
        content = f.read()
    
    enum_match = re.search(
        r'(?:#\[contracterror\].*?)?pub enum ' + enum_name + r' \{(.*?)\}',
        content,
        re.DOTALL
    )
    
    if not enum_match:
        return []
    
    enum_body = enum_match.group(1)
    error_pattern = r'(\w+)\s*=\s*(\d+)'
    errors = []
    
    for match in re.finditer(error_pattern, enum_body):
        name = match.group(1)
        code = int(match.group(2))
        errors.append((code, name))
    
    errors.sort(key=lambda x: x[0])
    return errors


def generate_markdown_table(instance_errors, factory_errors, protocol_errors):
    """Generate markdown table from error lists."""
    lines = []
    lines.append("| Code | Error | Description | Frontend Message |")
    lines.append("| ---- | ----- | ----------- | ---------------- |")
    
    descriptions = {
        'RaffleNotFound': 'The raffle data was not found in storage',
        'RaffleInactive': 'The raffle is not in an active state',
        'TicketsSoldOut': 'All tickets have been sold',
        'InsufficientFunds': 'User does not have enough balance',
        'NotAuthorized': 'User is not authorized to perform this action',
        'OracleNotSet': 'Oracle address is not configured',
        'RandomnessAlreadyRequested': 'Randomness has already been requested',
        'NoRandomnessRequest': 'No randomness request found',
        'FallbackTooEarly': 'Fallback randomness triggered too early',
        'PrizeNotDeposited': 'Prize has not been deposited yet',
        'PrizeAlreadyClaimed': 'Prize has already been claimed',
        'PrizeAlreadyDeposited': 'Prize deposit was already completed',
        'NotWinner': 'Only the winner can claim the prize',
        'ClaimTooEarly': 'Cannot claim before cooldown period',
        'InvalidParameters': 'Invalid input parameters provided',
        'InvalidQuantity': 'Invalid ticket quantity requested',
        'InvalidStatus': 'The current raffle status doesn\'t allow this operation',
        'ContractPaused': 'The contract is paused',
        'InvalidStateTransition': 'Cannot transition to the requested state',
        'RaffleExpired': 'The raffle end time has passed',
        'InsufficientTickets': 'Not enough tickets sold to finalize',
        'MultipleTicketsNotAllowed': 'User already has a ticket',
        'NoTicketsSold': 'No tickets have been purchased',
        'TicketNotFound': 'The requested ticket was not found',
        'RaffleEnded': 'The raffle has already ended',
        'ArithmeticOverflow': 'Arithmetic operation overflow',
        'AlreadyInitialized': 'Contract is already initialized',
        'NotInitialized': 'Contract has not been initialized',
        'Reentrancy': 'Reentrant call detected',
        'TokenTransferFailed': 'Token transfer failed',
        'NoActiveTickets': 'No active tickets available',
        'DeadlinePassed': 'Swap deadline has passed',
        'SlippageExceeded': 'Slippage tolerance exceeded',
        'InvalidIndex': 'Invalid index provided',
        'MorePrizesThanTickets': 'More prizes than tickets',
        'ZeroPrize': 'Prize amount is zero',
        'InvalidTokenAddress': 'Invalid token address provided',
        'TooManyPrizes': 'Exceeds maximum number of prizes',
        'EmergencyTooEarly': 'Emergency withdraw too early',
        'InvalidTicketRange': 'Invalid ticket range configured',
        'InsufficientAccumulatedFees': 'Insufficient accumulated fees',
        'PrizeConfigurationLocked': 'Prize configuration is locked',
        'ExceedsMaxTicketsPerTx': 'Exceeds max tickets per transaction',
        'DrawingAlreadyInProgress': 'A draw is already in progress',
        'DrawingAlreadyComplete': 'Randomness was already provided',
        'InvalidEndTime': 'Raffle end time is invalid',
        'InvalidAdminAddress': 'Admin address is invalid',
        'InvalidStatusForDrawingTransition': 'Raffle status cannot enter Drawing',
        'RandomnessTooEarly': 'Randomness request too early',
        'InvalidRaffleId': 'Invalid raffle stable-ID',
        'RaffleNotEligible': 'Raffle is not eligible for this operation',
        'TreasuryNotSet': 'Treasury address is not configured',
        'FactoryAlreadyInitialized': 'Factory is already initialized',
        'FactoryNotAuthorized': 'Caller is not the factory admin',
        'FactoryContractPaused': 'Factory is paused',
        'FactoryInvalidParameters': 'Invalid factory parameters',
        'FactoryRaffleNotFound': 'Raffle instance not found in factory',
        'FactoryAdminTransferPending': 'Admin transfer already pending',
        'FactoryNoPendingTransfer': 'No pending admin transfer',
        'FactoryRateLimitExceeded': 'Raffle creation rate limit exceeded',
        'FactoryNoPendingOp': 'No pending operation',
        'FactoryTimelockNotElapsed': 'Timelock delay has not elapsed',
        'FactoryInvalidRaffleId': 'Invalid raffle stable-ID',
        'FactoryRaffleNotEligible': 'Raffle is not eligible for this operation',
        'FactoryArithmeticOverflow': 'Arithmetic overflow in factory',
        'FactoryTreasuryNotSet': 'Factory treasury address not configured',
    }
    
    messages = {
        'RaffleNotFound': '"Raffle not found"',
        'RaffleInactive': '"This raffle is not currently active"',
        'TicketsSoldOut': '"Sorry, all tickets have been sold!"',
        'InsufficientFunds': '"Insufficient funds to complete this action"',
        'NotAuthorized': '"You are not authorized to perform this action"',
        'OracleNotSet': '"Oracle address is not set"',
        'RandomnessAlreadyRequested': '"Randomness request already in progress"',
        'NoRandomnessRequest': '"No randomness request found"',
        'FallbackTooEarly': '"Fallback randomness not available yet"',
        'PrizeNotDeposited': '"Prize not yet deposited"',
        'PrizeAlreadyClaimed': '"Prize has already been claimed"',
        'PrizeAlreadyDeposited': '"Prize has already been deposited"',
        'NotWinner': '"You are not the winner of this raffle"',
        'ClaimTooEarly': '"Please wait before claiming your prize"',
        'InvalidParameters': '"Invalid parameters provided"',
        'InvalidQuantity': '"Invalid ticket quantity"',
        'InvalidStatus': '"This action is not allowed in the current raffle state"',
        'ContractPaused': '"Contract is temporarily paused"',
        'InvalidStateTransition': '"Cannot change raffle to the requested state"',
        'RaffleExpired': '"This raffle has ended"',
        'InsufficientTickets': '"Minimum ticket requirement not met"',
        'MultipleTicketsNotAllowed': '"Multiple tickets not allowed for this raffle"',
        'NoTicketsSold': '"No tickets have been sold yet"',
        'TicketNotFound': '"Ticket not found"',
        'RaffleEnded': '"This raffle has already ended"',
        'ArithmeticOverflow': '"Calculation error occurred"',
        'AlreadyInitialized': '"Contract already initialized"',
        'NotInitialized': '"Contract not initialized"',
        'Reentrancy': '"Please try again later"',
        'TokenTransferFailed': '"Token transfer failed"',
        'NoActiveTickets': '"No active tickets available"',
        'DeadlinePassed': '"Swap deadline has passed"',
        'SlippageExceeded': '"Slippage tolerance exceeded"',
        'InvalidIndex': '"Invalid index provided"',
        'MorePrizesThanTickets': '"More prizes than tickets"',
        'ZeroPrize': '"Prize amount cannot be zero"',
        'InvalidTokenAddress': '"Invalid token address"',
        'TooManyPrizes': '"Too many prizes configured"',
        'EmergencyTooEarly': '"Emergency withdraw not available yet"',
        'InvalidTicketRange': '"Invalid ticket range"',
        'InsufficientAccumulatedFees': '"Insufficient accumulated fees"',
        'PrizeConfigurationLocked': '"Prize configuration is locked"',
        'ExceedsMaxTicketsPerTx': '"Too many tickets for one transaction"',
        'DrawingAlreadyInProgress': '"Drawing already in progress"',
        'DrawingAlreadyComplete': '"Drawing already complete"',
        'InvalidEndTime': '"Invalid raffle end time"',
        'InvalidAdminAddress': '"Invalid admin address"',
        'InvalidStatusForDrawingTransition': '"Cannot start drawing in current state"',
        'RandomnessTooEarly': '"Randomness request too early"',
        'InvalidRaffleId': '"Invalid raffle ID"',
        'RaffleNotEligible': '"Raffle is not eligible for this operation"',
        'TreasuryNotSet': '"Treasury address is not set"',
        'FactoryAlreadyInitialized': '"Factory already initialized"',
        'FactoryNotAuthorized': '"You are not the factory admin"',
        'FactoryContractPaused': '"Factory is temporarily paused"',
        'FactoryInvalidParameters': '"Invalid factory parameters provided"',
        'FactoryRaffleNotFound': '"Raffle not found in factory registry"',
        'FactoryAdminTransferPending': '"Admin transfer already pending"',
        'FactoryNoPendingTransfer': '"No pending admin transfer"',
        'FactoryRateLimitExceeded': '"Raffle creation rate limit exceeded"',
        'FactoryNoPendingOp': '"No pending operation"',
        'FactoryTimelockNotElapsed': '"Timelock delay has not elapsed"',
        'FactoryInvalidRaffleId': '"Invalid raffle ID"',
        'FactoryRaffleNotEligible': '"Raffle is not eligible for this operation"',
        'FactoryArithmeticOverflow': '"Calculation error in factory"',
        'FactoryTreasuryNotSet': '"Treasury address is not set"',
    }
    
    all_errors = []
    seen = set()
    for code, name in instance_errors:
        if code not in seen:
            seen.add(code)
            all_errors.append((code, name, 'instance'))
    for code, name in factory_errors:
        if code not in seen:
            seen.add(code)
            all_errors.append((code, name, 'factory'))
    for code, name in protocol_errors:
        if code not in seen:
            seen.add(code)
            all_errors.append((code, name, 'protocol'))
    
    all_errors.sort(key=lambda x: x[0])
    
    for code, name, source in all_errors:
        desc = descriptions.get(name, 'TODO: Add description')
        msg = messages.get(name, 'TODO: Add message')
        source_label = f" ({source})" if source != 'instance' else ""
        lines.append(f"| {code} | `{name}`{source_label} | {desc} | {msg} |")
    
    return '\n'.join(lines)


def generate_typescript_mapping(instance_errors, factory_errors, protocol_errors):
    """Generate TypeScript error mapping from error lists."""
    lines = []
    lines.append("const errorMessages: Record<number, string> = {")
    
    messages = {
        'RaffleNotFound': 'Raffle not found',
        'RaffleInactive': 'This raffle is not currently active',
        'TicketsSoldOut': 'Sorry, all tickets have been sold!',
        'InsufficientFunds': 'Insufficient funds to complete this action',
        'NotAuthorized': 'You are not authorized to perform this action',
        'OracleNotSet': 'Oracle address is not set',
        'RandomnessAlreadyRequested': 'Randomness request already in progress',
        'NoRandomnessRequest': 'No randomness request found',
        'FallbackTooEarly': 'Fallback randomness not available yet',
        'PrizeNotDeposited': 'Prize not yet deposited',
        'PrizeAlreadyClaimed': 'Prize has already been claimed',
        'PrizeAlreadyDeposited': 'Prize has already been deposited',
        'NotWinner': 'You are not the winner of this raffle',
        'ClaimTooEarly': 'Please wait before claiming your prize',
        'InvalidParameters': 'Invalid parameters provided',
        'InvalidQuantity': 'Invalid ticket quantity',
        'InvalidStatus': 'This action is not allowed in the current raffle state',
        'ContractPaused': 'Contract is temporarily paused',
        'InvalidStateTransition': 'Cannot change raffle to the requested state',
        'RaffleExpired': 'This raffle has ended',
        'InsufficientTickets': 'Minimum ticket requirement not met',
        'MultipleTicketsNotAllowed': 'Multiple tickets not allowed for this raffle',
        'NoTicketsSold': 'No tickets have been sold yet',
        'TicketNotFound': 'Ticket not found',
        'RaffleEnded': 'This raffle has already ended',
        'ArithmeticOverflow': 'Calculation error occurred',
        'AlreadyInitialized': 'Contract already initialized',
        'NotInitialized': 'Contract not initialized',
        'Reentrancy': 'Please try again later',
        'TokenTransferFailed': 'Token transfer failed',
        'NoActiveTickets': 'No active tickets available',
        'DeadlinePassed': 'Swap deadline has passed',
        'SlippageExceeded': 'Slippage tolerance exceeded',
        'InvalidIndex': 'Invalid index provided',
        'MorePrizesThanTickets': 'More prizes than tickets',
        'ZeroPrize': 'Prize amount cannot be zero',
        'InvalidTokenAddress': 'Invalid token address',
        'TooManyPrizes': 'Too many prizes configured',
        'EmergencyTooEarly': 'Emergency withdraw not available yet',
        'InvalidTicketRange': 'Invalid ticket range',
        'InsufficientAccumulatedFees': 'Insufficient accumulated fees',
        'PrizeConfigurationLocked': 'Prize configuration is locked',
        'ExceedsMaxTicketsPerTx': 'Too many tickets for one transaction',
        'DrawingAlreadyInProgress': 'Drawing already in progress',
        'DrawingAlreadyComplete': 'Drawing already complete',
        'InvalidEndTime': 'Invalid raffle end time',
        'InvalidAdminAddress': 'Invalid admin address',
        'InvalidStatusForDrawingTransition': 'Cannot start drawing in current state',
        'RandomnessTooEarly': 'Randomness request too early',
        'InvalidRaffleId': 'Invalid raffle ID',
        'RaffleNotEligible': 'Raffle is not eligible for this operation',
        'TreasuryNotSet': 'Treasury address is not set',
        'FactoryAlreadyInitialized': 'Factory already initialized',
        'FactoryNotAuthorized': 'You are not the factory admin',
        'FactoryContractPaused': 'Factory is temporarily paused',
        'FactoryInvalidParameters': 'Invalid factory parameters provided',
        'FactoryRaffleNotFound': 'Raffle not found in factory registry',
        'FactoryAdminTransferPending': 'Admin transfer already pending',
        'FactoryNoPendingTransfer': 'No pending admin transfer',
        'FactoryRateLimitExceeded': 'Raffle creation rate limit exceeded',
        'FactoryNoPendingOp': 'No pending operation',
        'FactoryTimelockNotElapsed': 'Timelock delay has not elapsed',
        'FactoryInvalidRaffleId': 'Invalid raffle ID',
        'FactoryRaffleNotEligible': 'Raffle is not eligible for this operation',
        'FactoryArithmeticOverflow': 'Calculation error in factory',
        'FactoryTreasuryNotSet': 'Treasury address is not set',
    }
    
    all_errors = []
    seen = set()
    for code, name in instance_errors:
        if code not in seen:
            seen.add(code)
            all_errors.append((code, name))
    for code, name in factory_errors:
        if code not in seen:
            seen.add(code)
            all_errors.append((code, name))
    for code, name in protocol_errors:
        if code not in seen:
            seen.add(code)
            all_errors.append((code, name))
    
    all_errors.sort(key=lambda x: x[0])
    
    for code, name in all_errors:
        msg = messages.get(name, 'TODO: Add message')
        lines.append(f"  {code}: \"{msg}\",")
    
    lines.append("};")
    return '\n'.join(lines)


def main():
    repo_root = Path(__file__).parent.parent
    rust_file = repo_root / "contracts" / "raffle-instance" / "src" / "lib.rs"
    errors_doc = repo_root / "docs" / "ERRORS.md"
    
    if not rust_file.exists():
        print(f"Error: Rust file not found at {rust_file}")
        sys.exit(1)
    
    instance_errors = parse_error_enum(instance_file, "Error")
    factory_errors = parse_error_enum(factory_file, "ContractError")
    protocol_errors = parse_error_enum(shared_file, "ProtocolError")
    
    table_content = generate_markdown_table(errors)
    
    if errors_doc.exists():
        with open(errors_doc, 'r') as f:
            doc_text = f.read()
        
        # Replace content between Error Code Mapping header or update table section
        # Ensure deterministic file writing
        lines = doc_text.splitlines()
        new_lines = []
        in_table_section = False
        
        for line in lines:
            if line.startswith("### General Errors") or line.startswith("| Code | Error"):
                if not in_table_section:
                    in_table_section = True
                    new_lines.append(table_content)
                continue
            if in_table_section:
                if line.startswith("## ") or line.startswith("---"):
                    in_table_section = False
                    new_lines.append(line)
            else:
                new_lines.append(line)
        
        updated_doc = '\n'.join(new_lines) + '\n'
        with open(errors_doc, 'w') as f:
            f.write(updated_doc)
    else:
        with open(errors_doc, 'w') as f:
            f.write(table_content + '\n')


if __name__ == "__main__":
    main()

