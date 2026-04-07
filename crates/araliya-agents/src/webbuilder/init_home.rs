//! Static HTML/CSS/JS page generator for the homebuilder agent.
//!
//! Generates a welcome page with user profile setup, notes config, and workflow placeholders.
//! Skips if `index.html` already exists (idempotent).

use std::path::Path;

use araliya_core::error::AppError;

/// Generate and write static homebuilder welcome page files.
///
/// Creates:
/// - `dist/index.html` — welcome page markup
/// - `dist/style.css` — Seedling dark marine palette (--void/#080c0e bg, --ice/#4db8cc accent)
/// - `dist/app.js` — minimal interactivity
///
/// If `dist/index.html` already exists, this is a no-op (idempotent).
pub fn write_static_page(
    dist_dir: &Path,
    user_name: &str,
    notes_dir: Option<&str>,
    public_id: Option<&str>,
) -> Result<(), AppError> {
    let index_path = dist_dir.join("index.html");

    // Idempotent: skip if already exists
    if index_path.exists() {
        return Ok(());
    }

    // Ensure dist directory exists
    std::fs::create_dir_all(dist_dir)
        .map_err(|e| AppError::Config(format!("cannot create dist dir: {e}")))?;

    // Write index.html
    let html = generate_html(user_name, notes_dir, public_id);
    std::fs::write(&index_path, html)
        .map_err(|e| AppError::Config(format!("cannot write index.html: {e}")))?;

    // Write style.css
    let css_path = dist_dir.join("style.css");
    std::fs::write(&css_path, STYLE_CSS)
        .map_err(|e| AppError::Config(format!("cannot write style.css: {e}")))?;

    // Write app.js
    let js_path = dist_dir.join("app.js");
    std::fs::write(&js_path, APP_JS)
        .map_err(|e| AppError::Config(format!("cannot write app.js: {e}")))?;

    Ok(())
}

fn generate_html(user_name: &str, notes_dir: Option<&str>, public_id: Option<&str>) -> String {
    let user_section = if user_name.is_empty() && public_id.is_none() {
        r#"<div class="card">
      <h3>User Profile</h3>
      <p>No user identity yet. <button class="action-btn">Create Identity</button></p>
    </div>"#
            .to_string()
    } else {
        let name_html = if user_name.is_empty() {
            String::new()
        } else {
            format!("<p>Name: <strong>{}</strong></p>", escape_html(user_name))
        };
        let id_html = if let Some(pid) = public_id {
            format!(
                "<p class=\"pub-id\">ID: <code>{}</code></p>",
                escape_html(pid)
            )
        } else {
            String::new()
        };
        format!(
            r#"<div class="card">
      <h3>User Profile</h3>
      {name_html}{id_html}
      <button class="action-btn">Manage</button>
    </div>"#
        )
    };

    let notes_section = if let Some(dir) = notes_dir {
        format!(
            r#"<div class="card">
      <h3>Notes</h3>
      <p>Serving markdown from: <code>{}</code></p>
      <a href="/notes/" class="action-btn">Browse Notes</a>
    </div>"#,
            escape_html(dir)
        )
    } else {
        r#"<div class="card">
      <h3>Notes</h3>
      <p>No notes folder configured. Set <code>notes_dir</code> in homebuilder config to enable.</p>
    </div>"#
            .to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Araliya Home</title>
  <link rel="stylesheet" href="/home/style.css" />
</head>
<body>
  <div class="container">
    <header>
      <h1>Araliya</h1>
      <p class="subtitle">personal AI assistant &amp; knowledge platform</p>
    </header>

    <nav>
      <a href="/ui/" class="nav-link">Chat UI</a>
    </nav>

    <main>
      <section class="workflow">
        <h2>Quick Start</h2>

        {}

        {}

        <div class="card">
          <h3>Social Media</h3>
          <p>Download and sync your social media data (legally mandated archives). <strong>Coming soon.</strong></p>
        </div>

        <div class="card">
          <h3>Publishing</h3>
          <p>Publish content to your websites and domains. <strong>Coming soon.</strong></p>
        </div>
      </section>
    </main>

    <footer>
      <p>Araliya v0.2.0-alpha</p>
    </footer>
  </div>

  <script src="/home/app.js"></script>
</body>
</html>"#,
        user_section, notes_section
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE_CSS: &str = r#"/* Seedling palette */
:root {
  --void:   #080c0e;
  --deep:   #0d1317;
  --plate:  #111b1f;
  --seam:   #1c2d33;
  --hull:   #2a404a;
  --oxide:  #3d5a65;
  --haze:   #7a9aa5;
  --mist:   #b8ced6;
  --bone:   #e8eef0;
  --ice:    #4db8cc;
  --dew:    #7ed4e2;
  --frost:  #b2eaf2;
  --amber:  #c4892a;
  --signal: #2ac48a;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: 'JetBrains Mono', 'Fira Mono', 'Courier New', monospace;
  background: var(--void);
  color: var(--mist);
  line-height: 1.7;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.container {
  max-width: 900px;
  margin: 0 auto;
  padding: 2rem;
  flex: 1;
}

header {
  text-align: center;
  margin-bottom: 3rem;
  animation: fadeIn 0.6s ease-out;
}

header h1 {
  font-size: 3rem;
  margin-bottom: 0.5rem;
  color: var(--ice);
  text-shadow: 0 0 20px rgba(77, 184, 204, 0.25);
}

header .subtitle {
  font-size: 1.1rem;
  color: var(--haze);
}

nav {
  text-align: center;
  margin-bottom: 2rem;
}

.nav-link {
  display: inline-block;
  padding: 0.75rem 1.5rem;
  background: var(--ice);
  color: var(--void);
  text-decoration: none;
  border-radius: 4px;
  font-weight: 600;
  letter-spacing: 0.05em;
  transition: all 0.2s ease;
  box-shadow: 0 4px 15px rgba(77, 184, 204, 0.2);
}

.nav-link:hover {
  background: var(--dew);
  transform: translateY(-2px);
  box-shadow: 0 6px 20px rgba(77, 184, 204, 0.3);
}

main {
  flex: 1;
}

.workflow {
  display: grid;
  gap: 1.5rem;
}

.workflow h2 {
  font-size: 1.8rem;
  margin-bottom: 1rem;
  color: var(--bone);
}

.card {
  padding: 1.5rem;
  background: var(--plate);
  border: 1px solid var(--seam);
  border-radius: 4px;
  transition: all 0.2s ease;
  animation: slideUp 0.6s ease-out;
}

.card:hover {
  background: var(--deep);
  border-color: var(--hull);
  box-shadow: 0 4px 20px rgba(77, 184, 204, 0.08);
}

.card h3 {
  font-size: 1.1rem;
  margin-bottom: 0.75rem;
  color: var(--ice);
  letter-spacing: 0.04em;
}

.card p {
  margin-bottom: 1rem;
  color: var(--haze);
}

.pub-id code {
  font-size: 0.85em;
  color: var(--ice);
  letter-spacing: 0.08em;
}

.card code {
  background: var(--seam);
  padding: 0.2rem 0.4rem;
  border-radius: 2px;
  font-family: inherit;
  color: var(--dew);
}

.action-btn {
  padding: 0.5rem 1rem;
  background: var(--ice);
  color: var(--void);
  border: none;
  border-radius: 3px;
  cursor: pointer;
  font-weight: 600;
  font-family: inherit;
  letter-spacing: 0.05em;
  transition: all 0.2s ease;
}

.action-btn:hover {
  background: var(--dew);
  transform: translateY(-1px);
  box-shadow: 0 2px 8px rgba(77, 184, 204, 0.25);
}

.action-btn:active {
  transform: translateY(0);
}

footer {
  text-align: center;
  padding: 1rem;
  color: var(--oxide);
  border-top: 1px solid var(--seam);
  margin-top: 2rem;
  font-size: 0.8rem;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
"#;

const APP_JS: &str = r#"// Minimal interactivity for homebuilder welcome page
// Buttons are prepared for future endpoints

document.querySelectorAll('.action-btn').forEach(btn => {
  btn.addEventListener('click', async (e) => {
    const card = e.target.closest('.card');
    const h3 = card.querySelector('h3');
    const action = h3.textContent.trim();

    console.log(`Action clicked: ${action}`);
    // Future: call API endpoints here
    // e.g., POST /api/user/identity/create
  });
});

// Optional: add keyboard shortcuts
document.addEventListener('keydown', (e) => {
  if (e.key === '?' && !e.ctrlKey && !e.metaKey) {
    console.log('Help: Press ? again to close');
  }
});
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_static_page() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let dist = tmp.path().join("dist");

        write_static_page(&dist, "Alice", Some("/home/alice/notes"), Some("ab12cd34"))
            .expect("write page");

        assert!(dist.join("index.html").exists());
        assert!(dist.join("style.css").exists());
        assert!(dist.join("app.js").exists());

        let html = std::fs::read_to_string(dist.join("index.html")).expect("read html");
        assert!(html.contains("Alice"));
        assert!(html.contains("/home/alice/notes"));
        assert!(html.contains("ab12cd34"));
    }

    #[test]
    fn test_write_static_page_idempotent() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let dist = tmp.path().join("dist");

        write_static_page(&dist, "Alice", None, None).expect("first write");
        let html1 = std::fs::read_to_string(dist.join("index.html")).expect("read html");

        write_static_page(&dist, "Bob", None, None).expect("second write");
        let html2 = std::fs::read_to_string(dist.join("index.html")).expect("read html");

        // Should not change on second call
        assert_eq!(html1, html2);
        assert!(html1.contains("Alice"));
        assert!(!html1.contains("Bob"));
    }

    #[test]
    fn test_html_escaping() {
        let html = generate_html("<script>alert('xss')</script>", None, None);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
