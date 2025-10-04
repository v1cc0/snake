# Incident Log

## INCIDENT-001: API Keys Exposed in Git History (2025-10-05)

### Severity: CRITICAL 🔴

### Description
Multiple API keys (OpenAI, Groq) were accidentally committed to git history in the `config.toml` file across multiple commits. GitHub Push Protection blocked the push and detected the following secrets:
- OpenAI API Key in commits: 8223522, 424324d, ec24cc0
- Groq API Keys (2 keys) in commits: 8223522, 424324d, ec24cc0

### Root Cause
**Critical Mistake:** `config.toml` containing real API credentials was added to git BEFORE being added to `.gitignore`.

Timeline of the mistake:
1. Created `config.toml` with real credentials during TOML migration (commit 8223522)
2. Made multiple commits modifying `config.toml` with live API keys (commits ec24cc0, 424324d, d9f00e4, bec1e01, fe1ac6f, 7b5a1c0, 204b0fe, ef7da7f)
3. Only added `config.toml` to `.gitignore` in final cleanup commit (204b0fe)
4. Attempted to push, GitHub Push Protection detected secrets and rejected push

**Fundamental Error:** Violated the principle of "protect secrets FIRST, then create them."

### Impact
- ✅ **Mitigated:** Secrets were NOT pushed to remote repository (GitHub Push Protection worked)
- ⚠️ **Local Risk:** API keys existed in local git history for ~9 commits
- ⚠️ **Historical Pollution:** Required rewriting entire git history (60 commits)
- ⚠️ **Collaboration Impact:** Force push required, all commit hashes changed

### Resolution Steps
```bash
# 1. Unstage any pending changes
git reset HEAD config.toml

# 2. Remove config.toml from ALL git history using filter-branch
FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch --force --index-filter \
  'git rm --cached --ignore-unmatch config.toml' \
  --prune-empty --tag-name-filter cat -- --all

# 3. Clean up backup references and garbage collect
rm -rf .git/refs/original/
git reflog expire --expire=now --all
git gc --prune=now --aggressive

# 4. Force push to overwrite remote history
git push --force -u origin main

# 5. Verify config.toml is removed from history
git log --all --pretty=format: --name-only --diff-filter=A | grep config.toml
# Should return: "config.toml not found in git history"
```

### Prevention Measures (MANDATORY FOR ALL FUTURE WORK)

**CRITICAL RULES - NEVER VIOLATE:**

1. **⛔ ALWAYS add sensitive files to `.gitignore` BEFORE creating them**
   ```bash
   # CORRECT ORDER:
   echo "config.toml" >> .gitignore
   git add .gitignore
   git commit -m "chore: protect config.toml"
   # NOW it's safe to create config.toml
   cp config.toml.template config.toml
   ```

2. **⛔ NEVER commit files containing API keys, tokens, or passwords**
   - Use `.template` files with placeholders
   - Check `.gitignore` coverage BEFORE creating sensitive files
   - Run `git status` before EVERY commit to verify no secrets staged

3. **⛔ ALWAYS verify before pushing:**
   ```bash
   # Check what will be pushed
   git log origin/main..HEAD --stat
   git diff origin/main..HEAD --name-only
   # Look for: config.toml, .env, credentials.*, *.key, *.pem
   ```

4. **✅ Use templates for all configuration files:**
   - `config.toml.template` (placeholders) → committed to git
   - `config.toml` (real credentials) → NEVER committed, in .gitignore
   - Document this pattern in README.md

5. **✅ Pre-commit checklist:**
   - [ ] No `config.toml` in `git status`
   - [ ] No `.env` files in `git status`
   - [ ] No files with "key", "token", "secret" in name
   - [ ] No hardcoded credentials in source code

### Lessons Learned

1. **GitHub Push Protection is the last line of defense, not the first**
   - It saved us this time, but we should never rely on it
   - Local git history is still contaminated until cleaned

2. **Git history rewriting is destructive and disruptive**
   - Changed all commit hashes (60 commits rewritten)
   - Forces everyone to re-clone or reset --hard
   - Breaks any references to old commit hashes

3. **Order matters in security:**
   - Protect → Create → Use (CORRECT)
   - Create → Use → Protect (WRONG - too late!)

4. **Template-first approach is mandatory:**
   - Never create sensitive config files directly
   - Always start from `.template` files
   - Always verify `.gitignore` coverage first

### Action Items for Future

- [x] Add `config.toml` to `.gitignore`
- [x] Create `config.toml.template` with placeholders
- [x] Clean git history using `git filter-branch`
- [x] Force push to remote repository
- [x] Document incident in INCIDENTS.md
- [ ] **TODO:** Consider adding pre-commit hooks to prevent secrets
- [ ] **TODO:** Add section in README.md warning about credential protection
- [ ] **TODO:** Consider using tools like `git-secrets` or `gitleaks`

### References
- GitHub Push Protection: https://docs.github.com/code-security/secret-scanning/working-with-secret-scanning-and-push-protection
- Git Filter-Branch: https://git-scm.com/docs/git-filter-branch
- Detected commits with secrets: 8223522, ec24cc0, 424324d, d9f00e4, bec1e01, fe1ac6f, 7b5a1c0, 204b0fe, ef7da7f

---

**⚠️ WARNING TO ALL DEVELOPERS:**
After this incident, git history was rewritten. If you have a local clone:
```bash
# Option 1: Re-clone
git clone <repo-url>

# Option 2: Reset to new history
git fetch origin
git reset --hard origin/main
```
