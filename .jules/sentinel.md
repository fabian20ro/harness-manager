## 2026-04-06 - System Process Information Disclosure

**Vulnerability:** Information disclosure via unrestricted process list capture and cross-project leaks.

**Learning:** Shelling out to `ps` and capturing its output without strict project-root matching can expose sensitive system-wide process information. On Linux, `/proc` is a more robust and direct source for process information, but it is platform-specific.

**Prevention:**
1. Use direct system APIs or virtual filesystems (like `/proc`) instead of shelling out.
2. Implement strict filtering based on project-specific markers (e.g., project root path) to isolate process capture.
3. Handle platform-specific collection methods gracefully to maintain portability.
