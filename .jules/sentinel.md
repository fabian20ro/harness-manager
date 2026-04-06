## 2024-05-18 - [Path Traversal Fix]
**Vulnerability:** The application was vulnerable to path traversal because `PathBuf::join` was being called with user-supplied inputs (`project_id`, `tool`, `node_id`) in `src/storage.rs`, which allowed an attacker to break out of the intended directory structure.
**Learning:** `Path::new(name).file_name()` returns `None` for names ending in `.` or `..`, so it is not a sufficient defense on its own when falling back to the original string.
**Prevention:** Explicitly handle inputs consisting entirely of traversal tokens (like `..`) by ensuring `file_name()` results aren't bypassed or manually replacing `.`, `/`, and `\` tokens.
