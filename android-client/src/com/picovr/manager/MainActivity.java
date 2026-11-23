package com.picovr.manager;

import android.Manifest;
import android.app.Activity;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.MediaRecorder;
import android.os.Build;
import android.os.Bundle;
import android.text.InputType;
import android.util.DisplayMetrics;
import android.view.Gravity;
import android.view.View;
import android.view.WindowManager;
import android.webkit.JavascriptInterface;
import android.webkit.PermissionRequest;
import android.webkit.WebChromeClient;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.Socket;
import java.net.URL;
import java.net.URLEncoder;
import java.util.Random;
import java.util.concurrent.atomic.AtomicBoolean;

public class MainActivity extends Activity {
    private static final String PREFS = "picovr_remote_prefs";
    private static final String KEY_HOST = "remote_host";
    private static final String KEY_PORT = "remote_port";

    private FrameLayout rootLayout;
    private ScrollView connectScroll;
    private LinearLayout panel;
    private WebView webView;
    private EditText ipInput;
    private EditText portInput;
    private TextView statusText;
    private Button connectBtn;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        enableEdgeToEdgeFullscreen();

        rootLayout = new FrameLayout(this);
        rootLayout.setBackgroundColor(Color.parseColor("#0B1120"));

        setupWebView();
        setupConnectView();

        rootLayout.addView(webView);
        rootLayout.addView(connectScroll);
        setContentView(rootLayout);

        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 101);
        }

        SharedPreferences sp = getSharedPreferences(PREFS, MODE_PRIVATE);
        String savedHost = sp.getString(KEY_HOST, "");
        String savedPort = sp.getString(KEY_PORT, "8999");
        ipInput.setText(savedHost);
        portInput.setText(savedPort);

        if (!savedHost.isEmpty()) {
            tryConnect(savedHost, savedPort, true);
        }
    }

    private void setupWebView() {
        webView = new WebView(this);
        webView.setVisibility(View.GONE);
        webView.setBackgroundColor(Color.parseColor("#0B1120"));
        WebSettings ws = webView.getSettings();
        ws.setJavaScriptEnabled(true);
        ws.setDomStorageEnabled(true);
        ws.setDatabaseEnabled(true);
        ws.setMediaPlaybackRequiresUserGesture(false);
        ws.setUseWideViewPort(false);
        ws.setLoadWithOverviewMode(false);
        ws.setSupportZoom(false);
        ws.setBuiltInZoomControls(false);
        ws.setDisplayZoomControls(false);
        ws.setTextZoom(100);
        ws.setCacheMode(WebSettings.LOAD_NO_CACHE);
        webView.clearCache(true);
        webView.addJavascriptInterface(new AndroidVoiceBridge(), "AndroidVoice");
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public void onReceivedError(WebView view, int errorCode, String description, String failingUrl) {
                runOnUiThread(() -> {
                    webView.setVisibility(View.GONE);
                    connectScroll.setVisibility(View.VISIBLE);
                    statusText.setText("连接中断: " + description);
                    statusText.setTextColor(Color.parseColor("#FB7185"));
                });
            }
        });
        webView.setWebChromeClient(new WebChromeClient() {
            @Override
            public void onPermissionRequest(PermissionRequest request) {
                runOnUiThread(() -> request.grant(request.getResources()));
            }
        });
    }

    private int computePanelWidthPx() {
        DisplayMetrics dm = getResources().getDisplayMetrics();
        boolean isLandscape = dm.widthPixels > dm.heightPixels;
        if (isLandscape) {
            return Math.min(dp(380), Math.round(dm.widthPixels * 0.52f));
        }
        return Math.min(dp(400), Math.round(dm.widthPixels * 0.90f));
    }

    private void applyPanelLayout() {
        if (panel == null) return;
        LinearLayout.LayoutParams cardLp = new LinearLayout.LayoutParams(
                computePanelWidthPx(), LinearLayout.LayoutParams.WRAP_CONTENT);
        cardLp.gravity = Gravity.CENTER;
        panel.setLayoutParams(cardLp);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) enableEdgeToEdgeFullscreen();
    }

    private void enableEdgeToEdgeFullscreen() {
        getWindow().setStatusBarColor(Color.TRANSPARENT);
        getWindow().setNavigationBarColor(Color.TRANSPARENT);
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams lp = getWindow().getAttributes();
            lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(lp);
        }
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().setDecorFitsSystemWindows(false);
        }
        getWindow().getDecorView().setSystemUiVisibility(
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        );
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        applyPanelLayout();
    }

    private void setupConnectView() {
        connectScroll = new ScrollView(this);
        connectScroll.setFillViewport(true);

        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setGravity(Gravity.CENTER);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setPadding(dp(18), dp(18), dp(18), dp(18));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(Color.parseColor("#131C31"));
        bg.setCornerRadius(dp(16));
        bg.setStroke(dp(1), Color.parseColor("#334155"));
        panel.setBackground(bg);

        TextView title = new TextView(this);
        title.setText("PicoVR-Manager");
        title.setTextColor(Color.WHITE);
        title.setTextSize(19);
        title.setTypeface(null, Typeface.BOLD);
        panel.addView(title);

        TextView sub = new TextView(this);
        sub.setText("远程服务控制端（仅连接局域网主控服务）");
        sub.setTextColor(Color.parseColor("#94A3B8"));
        sub.setTextSize(12);
        sub.setPadding(0, dp(2), 0, dp(12));
        panel.addView(sub);

        TextView ipLabel = new TextView(this);
        ipLabel.setText("主控端 IP 地址");
        ipLabel.setTextColor(Color.parseColor("#CBD5E1"));
        ipLabel.setTextSize(12);
        ipLabel.setPadding(0, 0, 0, dp(4));
        panel.addView(ipLabel);

        ipInput = createField("例如: 192.168.17.50", InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
        panel.addView(ipInput);

        TextView portLabel = new TextView(this);
        portLabel.setText("服务端口");
        portLabel.setTextColor(Color.parseColor("#CBD5E1"));
        portLabel.setTextSize(12);
        portLabel.setPadding(0, dp(10), 0, dp(4));
        panel.addView(portLabel);

        portInput = createField("默认 8999", InputType.TYPE_CLASS_NUMBER);
        panel.addView(portInput);

        statusText = new TextView(this);
        statusText.setTextColor(Color.parseColor("#94A3B8"));
        statusText.setTextSize(12);
        statusText.setPadding(0, dp(8), 0, dp(8));
        panel.addView(statusText);

        connectBtn = new Button(this);
        connectBtn.setText("🔗 连接远程控制中心");
        connectBtn.setTextColor(Color.WHITE);
        connectBtn.setTextSize(15);
        connectBtn.setTypeface(null, Typeface.BOLD);
        connectBtn.setAllCaps(false);
        connectBtn.setMinimumHeight(dp(44));
        GradientDrawable btnBg = new GradientDrawable();
        btnBg.setColor(Color.parseColor("#0EA5E9"));
        btnBg.setCornerRadius(dp(12));
        connectBtn.setBackground(btnBg);
        connectBtn.setOnClickListener(v -> {
            String host = ipInput.getText().toString().trim();
            String port = portInput.getText().toString().trim();
            if (port.isEmpty()) port = "8999";
            if (host.isEmpty()) {
                statusText.setText("请输入主控端 IP 地址");
                statusText.setTextColor(Color.parseColor("#FB7185"));
                return;
            }
            tryConnect(host, port, false);
        });
        panel.addView(connectBtn);

        container.addView(panel);
        applyPanelLayout();
        connectScroll.addView(container);
    }

    private EditText createField(String hint, int inputType) {
        EditText et = new EditText(this);
        et.setHint(hint);
        et.setHintTextColor(Color.parseColor("#64748B"));
        et.setTextColor(Color.WHITE);
        et.setTextSize(14);
        et.setSingleLine(true);
        et.setInputType(inputType);
        et.setPadding(dp(12), dp(10), dp(12), dp(10));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(Color.parseColor("#0F172A"));
        bg.setCornerRadius(dp(10));
        bg.setStroke(dp(1), Color.parseColor("#334155"));
        et.setBackground(bg);
        return et;
    }

    private void tryConnect(String host, String port, boolean silent) {
        connectBtn.setEnabled(false);
        statusText.setText("正在探测 " + host + ":" + port + " ...");
        statusText.setTextColor(Color.parseColor("#38BDF8"));

        new Thread(() -> {
            try {
                URL url = new URL("http://" + host + ":" + port + "/api/system/health");
                HttpURLConnection conn = (HttpURLConnection) url.openConnection();
                conn.setConnectTimeout(2500);
                conn.setReadTimeout(2500);
                int code = conn.getResponseCode();
                conn.disconnect();
                if (code == 200) {
                    getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                            .putString(KEY_HOST, host)
                            .putString(KEY_PORT, port)
                            .apply();
                    runOnUiThread(() -> {
                        connectBtn.setEnabled(true);
                        connectScroll.setVisibility(View.GONE);
                        webView.setVisibility(View.VISIBLE);
                        webView.clearCache(true);
                        webView.loadUrl("http://" + host + ":" + port + "/?client=android&t=" + System.currentTimeMillis());
                    });
                    return;
                }
                throw new RuntimeException("HTTP " + code);
            } catch (Exception e) {
                runOnUiThread(() -> {
                    connectBtn.setEnabled(true);
                    if (silent) {
                        statusText.setText("上次服务器离线，请确认主控端已开启或更换 IP");
                        statusText.setTextColor(Color.parseColor("#FBBF24"));
                    } else {
                        statusText.setText("连接失败: " + e.getMessage());
                        statusText.setTextColor(Color.parseColor("#FB7185"));
                    }
                });
            }
        }).start();
    }

    @Override
    public void onBackPressed() {
        if (webView.getVisibility() == View.VISIBLE && webView.canGoBack()) {
            webView.goBack();
        } else if (webView.getVisibility() == View.VISIBLE) {
            webView.setVisibility(View.GONE);
            connectScroll.setVisibility(View.VISIBLE);
            statusText.setText("已返回服务器连接配置");
            statusText.setTextColor(Color.parseColor("#94A3B8"));
        } else {
            super.onBackPressed();
        }
    }

    private int dp(int v) {
        return Math.round(v * getResources().getDisplayMetrics().density);
    }

    private final AtomicBoolean voiceRunning = new AtomicBoolean(false);
    private Thread voiceThread;

    private class AndroidVoiceBridge {
        @JavascriptInterface
        public boolean start(final String host, final int port, final String sn) {
            stop();
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                runOnUiThread(() -> requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 101));
                return false;
            }
            voiceRunning.set(true);
            voiceThread = new Thread(() -> runVoiceLoop(host, port, sn), "AndroidVoice");
            voiceThread.start();
            return true;
        }

        @JavascriptInterface
        public void stop() {
            voiceRunning.set(false);
            if (voiceThread != null) {
                voiceThread.interrupt();
                voiceThread = null;
            }
        }
    }

    private void runVoiceLoop(String host, int port, String sn) {
        AudioRecord recorder = null;
        Socket socket = null;
        try {
            int sampleRate = 16000;
            int minBuf = AudioRecord.getMinBufferSize(
                    sampleRate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT);
            int bufSize = Math.max(minBuf, 4096);
            recorder = new AudioRecord(
                    MediaRecorder.AudioSource.VOICE_COMMUNICATION,
                    sampleRate,
                    AudioFormat.CHANNEL_IN_MONO,
                    AudioFormat.ENCODING_PCM_16BIT,
                    bufSize);
            if (recorder.getState() != AudioRecord.STATE_INITIALIZED) {
                return;
            }

            socket = new Socket(host, port);
            socket.setTcpNoDelay(true);
            OutputStream out = socket.getOutputStream();
            InputStream in = socket.getInputStream();

            String path = "/ws/voice/" + URLEncoder.encode(sn, "UTF-8");
            String req = "GET " + path + " HTTP/1.1\r\n"
                    + "Host: " + host + ":" + port + "\r\n"
                    + "Upgrade: websocket\r\n"
                    + "Connection: Upgrade\r\n"
                    + "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
                    + "Sec-WebSocket-Version: 13\r\n\r\n";
            out.write(req.getBytes("UTF-8"));
            out.flush();

            // 读取 HTTP 101 握手响应头直到 \r\n\r\n
            int matched = 0;
            while (matched < 4) {
                int b = in.read();
                if (b < 0) return;
                if ((matched == 0 && b == '\r') || (matched == 1 && b == '\n')
                        || (matched == 2 && b == '\r') || (matched == 3 && b == '\n')) {
                    matched++;
                } else {
                    matched = (b == '\r') ? 1 : 0;
                }
            }

            recorder.startRecording();
            byte[] pcm = new byte[1280];
            byte[] mask = new byte[4];
            Random rnd = new Random();
            while (voiceRunning.get() && !Thread.currentThread().isInterrupted()) {
                int n = recorder.read(pcm, 0, pcm.length);
                if (n <= 0) continue;
                rnd.nextBytes(mask);
                if (n < 126) {
                    out.write(0x82);
                    out.write(0x80 | n);
                } else {
                    out.write(0x82);
                    out.write(0x80 | 126);
                    out.write((n >> 8) & 0xFF);
                    out.write(n & 0xFF);
                }
                out.write(mask);
                for (int i = 0; i < n; i++) {
                    out.write(pcm[i] ^ mask[i & 3]);
                }
                out.flush();
            }
        } catch (Throwable ignored) {
        } finally {
            try { if (recorder != null) { recorder.stop(); recorder.release(); } } catch (Throwable ignored) {}
            try { if (socket != null) socket.close(); } catch (Throwable ignored) {}
            voiceRunning.set(false);
        }
    }
}
