package org.ravet.gain;

import android.Manifest;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.pm.PackageManager;
import android.media.AudioAttributes;
import android.media.AudioDeviceInfo;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.midi.*;
import android.os.Bundle;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.widget.*;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Arrays;

public final class MainActivity extends Activity {
    static { System.loadLibrary("ravet_gain_mobile"); }
    private static native long create();
    private static native void destroy(long owner);
    private static native boolean start(long owner, int rate, int frames, int input, int output);
    private static native void stop(long owner);
    private static native boolean set(long owner, int id, float value);
    private static native String status(long owner);
    private static native int failure(long owner);
    private static native boolean midi(long owner, byte[] bytes, int length);
    private long owner;
    private int rate = 48000, frames = 256, input = 0, output = 0;
    private boolean running;
    private TextView status;
    private AudioManager manager;
    private AudioFocusRequest focus;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private MidiDevice midiDevice;
    private MidiOutputPort midiPort;
    private int midiGeneration;
    private final Object nativeLock = new Object();
    private final Runnable poll = new Runnable() {
        public void run() {
            if (owner != 0 && running) {
                String message = status(owner);
                if (failure(owner) != 0) stopAudio();
                status.setText(message);
            }
            handler.postDelayed(this, 500);
        }
    };
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved); owner = create(); manager = getSystemService(AudioManager.class);
        focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setOnAudioFocusChangeListener(change -> { if (change < 0) stopAudio(); }, handler).build();
        LinearLayout layout = new LinearLayout(this); layout.setOrientation(LinearLayout.VERTICAL); layout.setPadding(32, 32, 32, 32);
        status = new TextView(this); status.setText("Stopped · microphone monitoring follows selected output"); layout.addView(status);
        Button settings = new Button(this); settings.setText("Audio/MIDI settings"); settings.setOnClickListener(v -> settings()); layout.addView(settings);
        TextView label = new TextView(this); label.setText("Gain (0–4)"); layout.addView(label);
        SeekBar gain = new SeekBar(this); gain.setMax(400); gain.setProgress(100);
        gain.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            public void onProgressChanged(SeekBar b, int value, boolean user) { if (user && owner != 0) set(owner, 1, value / 100.0f); }
            public void onStartTrackingTouch(SeekBar b) {} public void onStopTrackingTouch(SeekBar b) {}
        }); layout.addView(gain);
        Switch bypass = new Switch(this); bypass.setText("Bypass"); bypass.setOnCheckedChangeListener((v, checked) -> { if (owner != 0) set(owner, 2, checked ? 1 : 0); }); layout.addView(bypass);
        Button start = new Button(this); start.setText("Start audio"); start.setOnClickListener(v -> startAudio()); layout.addView(start);
        Button stop = new Button(this); stop.setText("Stop audio"); stop.setOnClickListener(v -> stopAudio()); layout.addView(stop);
        ScrollView scroll = new ScrollView(this); scroll.addView(layout); setContentView(scroll); handler.post(poll);
    }
    private void startAudio() {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, 1); return;
        }
        if (manager.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) { status.setText("Audio focus unavailable"); return; }
        running = start(owner, rate, frames, input, output); status.setText(status(owner));
        if (!running) manager.abandonAudioFocusRequest(focus);
    }
    private void stopAudio() { if (owner != 0) stop(owner); running = false; manager.abandonAudioFocusRequest(focus); status.setText("Stopped"); }
    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request == 1 && results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) startAudio();
    }
    private void settings() {
        ArrayList<String> labels = new ArrayList<>(); ArrayList<Runnable> actions = new ArrayList<>();
        labels.add("Default input/output"); actions.add(() -> { input = output = 0; });
        for (AudioDeviceInfo device : manager.getDevices(AudioManager.GET_DEVICES_ALL)) {
            if (device.isSource()) { labels.add("Input: " + device.getProductName()); actions.add(() -> input = device.getId()); }
            if (device.isSink()) { labels.add("Output: " + device.getProductName()); actions.add(() -> output = device.getId()); }
        }
        for (int choice : new int[]{44100, 48000, 96000}) { labels.add(choice + " Hz (preferred)"); actions.add(() -> rate = choice); }
        for (int choice : new int[]{128, 256, 512}) { labels.add(choice + " frames (preferred)"); actions.add(() -> frames = choice); }
        MidiManager midi = getSystemService(MidiManager.class);
        if (midi != null) for (MidiDeviceInfo info : midiDevices(midi)) for (MidiDeviceInfo.PortInfo port : info.getPorts()) {
            if (port.getType() == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                labels.add("MIDI: " + info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME) + " " + port.getName());
                actions.add(() -> selectMidi(midi, info, port.getPortNumber()));
            }
        }
        labels.add("Disconnect MIDI"); actions.add(this::closeMidi);
        new AlertDialog.Builder(this).setTitle("Audio/MIDI · actual configuration in status")
            .setItems(labels.toArray(new String[0]), (dialog, which) -> { boolean restart = running; stopAudio(); actions.get(which).run(); if (restart) startAudio(); }).show();
    }
    @SuppressWarnings("deprecation")
    private static Collection<MidiDeviceInfo> midiDevices(MidiManager manager) {
        if (Build.VERSION.SDK_INT >= 33) return manager.getDevicesForTransport(MidiManager.TRANSPORT_MIDI_BYTE_STREAM);
        return Arrays.asList(manager.getDevices());
    }
    private void closeMidi() {
        MidiOutputPort port; MidiDevice device;
        synchronized (nativeLock) {
            ++midiGeneration; port = midiPort; device = midiDevice;
            midiPort = null; midiDevice = null;
        }
        try { if (port != null) port.close(); if (device != null) device.close(); } catch (IOException ignored) {}
    }
    private void selectMidi(MidiManager midi, MidiDeviceInfo info, int port) {
        closeMidi(); final int generation = midiGeneration;
        midi.openDevice(info, device -> {
            if (device == null) return;
            if (generation != midiGeneration || owner == 0) { try { device.close(); } catch (IOException ignored) {} return; }
            midiDevice = device; midiPort = device.openOutputPort(port);
            if (midiPort == null) { closeMidi(); return; }
            midiPort.connect(new MidiReceiver() {
                private final byte[] message = new byte[3]; private int used, required; private byte runningStatus;
                @Override public void onSend(byte[] bytes, int offset, int count, long timestamp) {
                    synchronized (nativeLock) {
                        if (owner == 0 || generation != midiGeneration) return;
                        for (int i = offset; i < offset + count; ++i) {
                            int b = bytes[i] & 255;
                            if (b >= 0xf8) { midi(owner, new byte[]{bytes[i]}, 1); continue; }
                            if (b >= 0x80) {
                                runningStatus = b < 0xf0 ? bytes[i] : 0; used = 0;
                                required = b < 0xf0 ? ((b & 0xe0) == 0xc0 ? 2 : 3) : b == 0xf1 || b == 0xf3 ? 2 : b == 0xf2 ? 3 : b == 0xf6 ? 1 : 0;
                                if (required == 0) continue;
                                message[used++] = bytes[i];
                            } else {
                                if (used == 0 && runningStatus != 0) { message[used++] = runningStatus; required = (runningStatus & 0xe0) == 0xc0 ? 2 : 3; }
                                if (used == 0 || required == 0) continue;
                                message[used++] = bytes[i];
                            }
                            if (used == required) { midi(owner, message, required); used = 0; }
                        }
                    }
                }
            });
        }, handler);
    }
    @Override protected void onPause() { stopAudio(); super.onPause(); }
    @Override protected void onDestroy() {
        handler.removeCallbacks(poll); closeMidi();
        synchronized (nativeLock) { destroy(owner); owner = 0; }
        super.onDestroy();
    }
}
