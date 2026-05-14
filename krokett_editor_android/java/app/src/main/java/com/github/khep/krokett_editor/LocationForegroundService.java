package com.github.khep.krokett_editor;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.IBinder;

import androidx.core.app.NotificationCompat;
import androidx.core.content.ContextCompat;

public class LocationForegroundService extends Service {
  private static final String CHANNEL_ID = "krokett_editor_location";
  private static final int NOTIFICATION_ID = 2001;

  public static void start(Context context) {
    Intent intent = new Intent(context, LocationForegroundService.class);
    ContextCompat.startForegroundService(context, intent);
  }

  public static void stop(Context context) {
    Intent intent = new Intent(context, LocationForegroundService.class);
    context.stopService(intent);
  }

  @Override
  public void onCreate() {
    super.onCreate();
    ensureNotificationChannel();
  }

  @Override
  public int onStartCommand(Intent intent, int flags, int startId) {
    Notification notification = new NotificationCompat.Builder(this, CHANNEL_ID)
        .setContentTitle("Krokett Editor")
        .setContentText("Suivi GPS actif")
        .setSmallIcon(android.R.drawable.ic_menu_mylocation)
        .setOngoing(true)
        .build();

    startForeground(NOTIFICATION_ID, notification);
    return START_STICKY;
  }

  @Override
  public void onDestroy() {
    stopForeground(true);
    super.onDestroy();
  }

  @Override
  public IBinder onBind(Intent intent) {
    return null;
  }

  private void ensureNotificationChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      return;
    }

    NotificationManager manager = getSystemService(NotificationManager.class);
    if (manager == null) {
      return;
    }

    NotificationChannel channel = new NotificationChannel(
        CHANNEL_ID,
        "Suivi GPS",
        NotificationManager.IMPORTANCE_LOW
    );
    channel.setDescription("Maintient la geolocalisation active en arriere-plan");
    manager.createNotificationChannel(channel);
  }
}
