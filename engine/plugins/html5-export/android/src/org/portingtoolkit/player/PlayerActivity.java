package org.portingtoolkit.player;

import android.app.Activity;
import android.net.Uri;
import android.os.Bundle;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;

/**
 * Plays an HTML5 game stored in the APK's assets/www/ in a full-screen
 * WebView. Files are served from https://appassets.androidplatform.net/www/,
 * a secure origin, so saves (localStorage), audio and WebGL work like on a
 * web host. Made by Porting Toolkit.
 */
public class PlayerActivity extends Activity {
    private static final String HOST = "appassets.androidplatform.net";
    private static final Map<String, String> MIME = new HashMap<>();

    static {
        String[] types = {
            "html", "text/html", "htm", "text/html", "js", "text/javascript", "mjs", "text/javascript",
            "json", "application/json", "css", "text/css", "txt", "text/plain", "csv", "text/csv",
            "xml", "application/xml", "svg", "image/svg+xml", "png", "image/png", "jpg", "image/jpeg",
            "jpeg", "image/jpeg", "gif", "image/gif", "webp", "image/webp", "bmp", "image/bmp",
            "ico", "image/x-icon", "ogg", "audio/ogg", "oga", "audio/ogg", "opus", "audio/ogg",
            "m4a", "audio/mp4", "mp3", "audio/mpeg", "wav", "audio/wav", "mp4", "video/mp4",
            "webm", "video/webm", "ogv", "video/ogg", "woff", "font/woff", "woff2", "font/woff2",
            "ttf", "font/ttf", "otf", "font/otf", "wasm", "application/wasm",
        };
        for (int i = 0; i < types.length; i += 2) {
            MIME.put(types[i], types[i + 1]);
        }
    }

    private WebView web;
    private Map<String, String> lowerCase;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
                | WindowManager.LayoutParams.FLAG_FULLSCREEN);
        web = new WebView(this);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setDatabaseEnabled(true);
        s.setMediaPlaybackRequiresUserGesture(false);
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        web.setWebChromeClient(new WebChromeClient());
        web.setWebViewClient(new WebViewClient() {
            @Override
            public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                return serve(request.getUrl());
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                // The game stays inside its own pages.
                return !HOST.equals(request.getUrl().getHost());
            }
        });
        setContentView(web);
        hideSystemBars();
        if (state != null) {
            web.restoreState(state);
        } else {
            web.loadUrl("https://" + HOST + "/www/" + readAsset("ptk-entry.txt", "index.html").trim());
        }
    }

    private String readAsset(String name, String fallback) {
        try (InputStream in = getAssets().open(name)) {
            BufferedReader r = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
            StringBuilder sb = new StringBuilder();
            String line;
            while ((line = r.readLine()) != null) {
                sb.append(line).append('\n');
            }
            return sb.toString();
        } catch (IOException e) {
            return fallback;
        }
    }

    /** Games made on Windows often name files with other letter cases. */
    private String findIgnoringCase(String path) {
        if (lowerCase == null) {
            lowerCase = new HashMap<>();
            for (String line : readAsset("ptk-files.txt", "").split("\n")) {
                if (!line.isEmpty()) {
                    lowerCase.put(line.toLowerCase(Locale.ROOT), line);
                }
            }
        }
        return lowerCase.get(path.toLowerCase(Locale.ROOT));
    }

    private WebResourceResponse serve(Uri uri) {
        if (!HOST.equals(uri.getHost())) {
            return null;
        }
        String path = uri.getPath();
        if (path == null || !path.startsWith("/www/") || path.contains("..")) {
            return null;
        }
        String asset = path.substring(1);
        InputStream in;
        try {
            in = getAssets().open(asset);
        } catch (IOException e) {
            String other = findIgnoringCase(asset);
            try {
                if (other == null) {
                    throw e;
                }
                in = getAssets().open(other);
            } catch (IOException missing) {
                return new WebResourceResponse("text/plain", "utf-8", 404, "Not Found", null, null);
            }
        }
        String ext = asset.substring(asset.lastIndexOf('.') + 1).toLowerCase(Locale.ROOT);
        String mime = MIME.containsKey(ext) ? MIME.get(ext) : "application/octet-stream";
        WebResourceResponse response = new WebResourceResponse(mime, mime.startsWith("text/") ? "utf-8" : null, in);
        Map<String, String> headers = new HashMap<>();
        headers.put("Access-Control-Allow-Origin", "*");
        headers.put("Cache-Control", "no-cache");
        response.setResponseHeaders(headers);
        return response;
    }

    @SuppressWarnings("deprecation")
    private void hideSystemBars() {
        getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    @Override
    protected void onPause() {
        web.onPause();
        super.onPause();
    }

    @Override
    protected void onResume() {
        super.onResume();
        web.onResume();
    }

    @Override
    protected void onSaveInstanceState(Bundle state) {
        super.onSaveInstanceState(state);
        web.saveState(state);
    }

    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        if (web.canGoBack()) {
            web.goBack();
        } else {
            super.onBackPressed();
        }
    }
}
