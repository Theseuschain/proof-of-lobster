//! Authentication flows for OAuth and magic link.

use anyhow::Result;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Auth method selection
#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    Twitter,
    Email(String),
}

/// Run the OAuth flow by opening a browser and waiting for callback.
pub async fn run_oauth_flow(server_url: &str, method: AuthMethod) -> Result<String> {
    // Bind to random available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // Get OAuth URL from server based on method
    let client = reqwest::Client::new();
    match method {
        AuthMethod::Twitter => {
            let auth_url: String = client
                .get(&format!("{}/auth/url?redirect_port={}&provider=twitter", server_url, port))
                .send()
                .await?
                .json()
                .await?;
            
            // Open browser for OAuth
            open::that(&auth_url)?;
        }
        AuthMethod::Email(ref email) => {
            // For email, request magic link to be sent
            let resp = client
                .post(&format!("{}/auth/magic-link", server_url))
                .json(&serde_json::json!({
                    "email": email,
                    "redirect_port": port
                }))
                .send()
                .await?;
            
            if !resp.status().is_success() {
                let error: serde_json::Value = resp.json().await.unwrap_or_default();
                anyhow::bail!("Failed to send magic link: {}", error.get("error").and_then(|e| e.as_str()).unwrap_or("Unknown error"));
            }
            
            // Don't open browser - user will click link in email
        }
    };

    // Wait for callback with timeout (10 minutes for email)
    wait_for_callback_with_fragment(listener).await
}

/// Wait for callback and handle URL fragment extraction.
/// 
/// URL fragments (#access_token=...) are NOT sent to the server in HTTP requests.
/// They only exist client-side in the browser. So we need to:
/// 1. Serve an HTML page with JavaScript that reads the fragment
/// 2. JavaScript redirects to us with the token in the query string
/// 3. We read the token from the query string
async fn wait_for_callback_with_fragment(listener: TcpListener) -> Result<String> {
    // Load the lobster image once
    let lobster_image = load_lobster_image();
    
    let token = tokio::time::timeout(Duration::from_secs(600), async {
        loop {
            let (mut socket, _) = listener.accept().await?;

            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await?;
            let request = String::from_utf8_lossy(&buf[..n]);
            let first_line = request.lines().next().unwrap_or("");

            // Check what's being requested
            if first_line.contains("GET /lobster.png") {
                // Serve the lobster image
                send_lobster_image(&mut socket, &lobster_image).await?;
            } else if let Some(token) = try_parse_token_from_query(&request) {
                // Got the token! Send success page.
                send_success_response(&mut socket).await?;
                return Ok::<_, anyhow::Error>(token);
            } else {
                // Initial callback - serve the fragment extractor page
                send_fragment_extractor(&mut socket).await?;
                // Continue waiting for the redirect with the token
            }
        }
    })
    .await??;

    Ok(token)
}

/// Load the lobster image from app folder
fn load_lobster_image() -> Vec<u8> {
    // Try to load from various possible paths
    let paths = [
        "pol.png",
        "app/pol.png",
    ];
    
    for path in paths {
        if let Ok(data) = std::fs::read(path) {
            return data;
        }
    }
    
    // Return empty if not found - we'll use emoji fallback
    Vec::new()
}

/// Serve the lobster PNG image
async fn send_lobster_image(socket: &mut tokio::net::TcpStream, image_data: &[u8]) -> Result<()> {
    if image_data.is_empty() {
        // 404 if no image
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        socket.write_all(response.as_bytes()).await?;
    } else {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nCache-Control: max-age=3600\r\nConnection: close\r\n\r\n",
            image_data.len()
        );
        socket.write_all(response.as_bytes()).await?;
        socket.write_all(image_data).await?;
    }
    socket.flush().await?;
    Ok(())
}

/// Try to parse token from query string (not fragment)
fn try_parse_token_from_query(request: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    
    // Look for: GET /callback?access_token=xxx or GET /token?access_token=xxx
    if !first_line.contains("access_token=") {
        return None;
    }
    
    let path_start = first_line.find('/')?;
    let path_end = first_line.rfind(" HTTP")?;
    let path = &first_line[path_start..path_end];

    // Extract query string (after ?)
    let query = path.split('?').nth(1)?;
    
    // Parse query params
    for param in query.split('&') {
        if let Some(value) = param.strip_prefix("access_token=") {
            return urlencoding::decode(value).ok().map(|s| s.into_owned());
        }
    }
    
    None
}

/// Send HTML page that extracts the URL fragment and redirects with query params
async fn send_fragment_extractor(socket: &mut tokio::net::TcpStream) -> Result<()> {
    // This page runs JavaScript to:
    // 1. Read the URL fragment (which contains the token)
    // 2. Redirect to the same server with the token in the query string
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Authenticating...</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #e74c3c 0%, #c0392b 100%);
            color: white;
        }
        .container {
            text-align: center;
            padding: 2rem;
        }
        .lobster { 
            width: 150px; 
            height: 150px; 
            margin-bottom: 1rem;
            image-rendering: pixelated;
        }
        h1 { font-size: 1.5rem; margin-bottom: 1rem; }
        p { font-size: 1rem; opacity: 0.9; }
        .error { color: #ffcccc; }
    </style>
</head>
<body>
    <div class="container">
        <img src="/lobster.png" alt="🦞" class="lobster" onerror="this.style.display='none';this.nextElementSibling.style.display='block'">
        <div style="font-size: 6rem; margin-bottom: 1rem; display: none;">🦞</div>
        <h1 id="status">Completing authentication...</h1>
        <p id="message">Please wait...</p>
    </div>
    <script>
        (function() {
            // Get the fragment (everything after #)
            var hash = window.location.hash.substring(1);
            
            if (!hash) {
                document.getElementById('status').textContent = 'Authentication Error';
                document.getElementById('message').textContent = 'No authentication data received. Please try again.';
                document.getElementById('message').className = 'error';
                return;
            }
            
            // Parse the fragment to get access_token
            var params = new URLSearchParams(hash);
            var accessToken = params.get('access_token');
            
            if (!accessToken) {
                document.getElementById('status').textContent = 'Authentication Error';
                document.getElementById('message').textContent = 'No access token found. Please try again.';
                document.getElementById('message').className = 'error';
                return;
            }
            
            // Redirect to the same server with token in query string
            // This allows the server to read it
            window.location.href = '/token?access_token=' + encodeURIComponent(accessToken);
        })();
    </script>
</body>
</html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );

    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

/// Send success HTML response to browser
async fn send_success_response(socket: &mut tokio::net::TcpStream) -> Result<()> {
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Authentication Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #e74c3c 0%, #c0392b 100%);
            color: white;
        }
        .container {
            text-align: center;
            padding: 2rem;
        }
        .lobster { 
            width: 150px; 
            height: 150px; 
            margin-bottom: 1rem;
            image-rendering: pixelated;
        }
        h1 { font-size: 2rem; margin-bottom: 1rem; }
        p { font-size: 1.1rem; opacity: 0.9; }
    </style>
</head>
<body>
    <div class="container">
        <img src="/lobster.png" alt="🦞" class="lobster" onerror="this.style.display='none';this.nextElementSibling.style.display='block'">
        <div style="font-size: 6rem; margin-bottom: 1rem; display: none;">🦞</div>
        <h1>Authentication Successful!</h1>
        <p>You can close this tab and return to the terminal.</p>
    </div>
</body>
</html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );

    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_query() {
        let request = "GET /token?access_token=test456&expires_in=3600 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(try_parse_token_from_query(request).unwrap(), "test456");
    }
    
    #[test]
    fn test_parse_no_token() {
        let request = "GET /callback HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert!(try_parse_token_from_query(request).is_none());
    }
}
