#!/bin/bash
# NoteCraft PII Scanner - Checks for personally identifiable information
# Run: bash SKILLS/nc-devx/scripts/check-pii.sh [--fix]

# PII patterns to detect
# Add project-specific patterns here. Examples (commented out for safety):
# PII_PATTERNS=(
#     "owner-name"                   # Your actual owner name
#     "owner@domain\.com"            # Your actual owner email
#     "company-domain"               # Your actual company domain
# )
# For now, we check for the most obvious dangers:
PII_PATTERNS=(
    "[0-9]{3}-[0-9]{2}-[0-9]{4}"   # SSN-like pattern (###-##-####)
)

FIX_MODE=false
if [[ "$1" == "--fix" ]]; then
    FIX_MODE=true
fi

# Determine what to scan
if git rev-parse --git-dir > /dev/null 2>&1; then
    # In a git repo: scan staged files if committing, all files if pushed
    if [[ -n "$1" ]] && [[ "$1" != "--fix" ]]; then
        # Called from pre-push: scan all changes
        FILES=$(git diff origin/main --name-only 2>/dev/null || git diff --cached --name-only)
    else
        # Called from pre-commit: scan staged files
        FILES=$(git diff --cached --name-only)
    fi
else
    # Not in git repo: scan current directory
    FILES=$(find . -type f -name "*.md" -o -name "*.rs" -o -name "*.sh" -o -name "*.ps1" | grep -v "\.git" | grep -v "target" | grep -v "node_modules")
fi

if [[ -z "$FILES" ]]; then
    echo "✓ No files to scan"
    exit 0
fi

FOUND_ISSUES=0
ISSUES_FILE=$(mktemp)

# Scan each file
while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    [[ ! -f "$file" ]] && continue

    # Skip binary files
    if file "$file" | grep -q "binary"; then
        continue
    fi

    # Check each pattern
    for pattern in "${PII_PATTERNS[@]}"; do
        if grep -q "$pattern" "$file" 2>/dev/null; then
            echo "$file: Found '$pattern'" >> "$ISSUES_FILE"
            FOUND_ISSUES=$((FOUND_ISSUES + 1))
        fi
    done
done <<< "$FILES"

if [[ $FOUND_ISSUES -gt 0 ]]; then
    echo "❌ PII Issues Found:"
    echo ""
    cat "$ISSUES_FILE"
    echo ""

    if [[ "$FIX_MODE" == true ]]; then
        echo "Attempting to fix PII issues..."
        # Note: Automated fixing is risky. Better to show and let user fix.
        # This just informs them of the issue.
        echo ""
        echo "To fix:"
        echo "  1. Edit the files listed above"
        echo "  2. Remove or replace sensitive data"
        echo "  3. Re-stage the changes: git add <file>"
        echo "  4. Re-commit or re-push"
    else
        echo "To auto-check before committing:"
        echo "  git commit  # Will run pre-commit hook"
        echo ""
        echo "To manually scan:"
        echo "  bash SKILLS/nc-devx/scripts/check-pii.sh"
        echo ""
        echo "To fix and retry:"
        echo "  1. Edit the files and remove PII"
        echo "  2. git add <file>"
        echo "  3. git commit (or git push)"
    fi

    rm -f "$ISSUES_FILE"
    exit 1
else
    echo "✓ No PII detected"
    rm -f "$ISSUES_FILE"
    exit 0
fi
