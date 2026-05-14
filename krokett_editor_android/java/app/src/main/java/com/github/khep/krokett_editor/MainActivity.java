package com.github.khep.krokett_editor;

import android.content.Intent;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Looper;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.provider.OpenableColumns;

import androidx.annotation.NonNull;
import androidx.core.app.ActivityCompat;

import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;

import com.google.androidgamesdk.GameActivity;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

public class MainActivity extends GameActivity {
  private static final int REQUEST_OPEN_GPX = 1001;
  private static final int REQUEST_SAVE_GPX = 1002;
  private static final int REQUEST_LOCATION_PERMISSION = 1003;
  private static final int REQUEST_BACKGROUND_LOCATION_PERMISSION = 1004;
  private static final long LOCATION_UPDATE_INTERVAL_MS = 5000L;

  private static MainActivity instance;
  private static byte[] pendingSaveData;
  private static LocationManager locationManager;
  private static LocationListener locationListener;
  private static boolean locationUpdatesActive;

  static {
    System.loadLibrary("main");
  }

  private static native void setAppInBackground(boolean isBackground);
  private static native void nativeOnGpxOpened(String name, byte[] data, String error);
  private static native void nativeOnGpxSaved(String fileName, String error);
  private static native void nativeOnLocationUpdated(double latitude, double longitude, String error);

  static void publishLocation(double latitude, double longitude, String error) {
    nativeOnLocationUpdated(latitude, longitude, error);
  }

  public static void requestDeviceLocation() {
    if (instance == null) {
      nativeOnLocationUpdated(Double.NaN, Double.NaN, "MainActivity indisponible");
      return;
    }

    instance.runOnUiThread(instance::requestOrFetchLocationOnUiThread);
  }

  private void requestOrFetchLocationOnUiThread() {
    if (ActivityCompat.checkSelfPermission(this, android.Manifest.permission.ACCESS_FINE_LOCATION)
        != PackageManager.PERMISSION_GRANTED
        && ActivityCompat.checkSelfPermission(this, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        != PackageManager.PERMISSION_GRANTED) {
      ActivityCompat.requestPermissions(
          this,
          new String[] {
              android.Manifest.permission.ACCESS_FINE_LOCATION,
              android.Manifest.permission.ACCESS_COARSE_LOCATION,
          },
          REQUEST_LOCATION_PERMISSION);
      return;
    }

    fetchLocation();
    requestBackgroundLocationPermissionIfNeeded();
  }

  private boolean hasBackgroundLocationPermission() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      return true;
    }

    return ActivityCompat.checkSelfPermission(this, android.Manifest.permission.ACCESS_BACKGROUND_LOCATION)
        == PackageManager.PERMISSION_GRANTED;
  }

  private void requestBackgroundLocationPermissionIfNeeded() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      return;
    }

    if (hasBackgroundLocationPermission()) {
      return;
    }

    ActivityCompat.requestPermissions(
        this,
        new String[] {
            android.Manifest.permission.ACCESS_BACKGROUND_LOCATION,
        },
        REQUEST_BACKGROUND_LOCATION_PERMISSION);
  }

  @SuppressWarnings("deprecation")
  private void fetchLocation() {
    try {
      locationManager = (LocationManager) getApplicationContext().getSystemService(LOCATION_SERVICE);
      if (locationManager == null) {
        publishLocation(Double.NaN, Double.NaN, "LocationManager indisponible");
        return;
      }

      Location lastKnown = locationManager.getLastKnownLocation(LocationManager.GPS_PROVIDER);
      if (lastKnown == null) {
        lastKnown = locationManager.getLastKnownLocation(LocationManager.NETWORK_PROVIDER);
      }

      if (lastKnown != null) {
        publishLocation(lastKnown.getLatitude(), lastKnown.getLongitude(), null);
      }

      if (locationListener == null) {
        locationListener = location -> publishLocation(location.getLatitude(), location.getLongitude(), null);
      }

      if (!locationUpdatesActive) {
        locationManager.requestLocationUpdates(
            LocationManager.GPS_PROVIDER,
            LOCATION_UPDATE_INTERVAL_MS,
            0f,
            locationListener,
            Looper.getMainLooper());
        locationManager.requestLocationUpdates(
            LocationManager.NETWORK_PROVIDER,
            LOCATION_UPDATE_INTERVAL_MS,
            0f,
            locationListener,
            Looper.getMainLooper());
        locationUpdatesActive = true;
      }

      LocationForegroundService.start(this);
    } catch (SecurityException e) {
      publishLocation(Double.NaN, Double.NaN, "Permission GPS refusee: " + e.getMessage());
    } catch (Exception e) {
      publishLocation(Double.NaN, Double.NaN, "Erreur geolocalisation: " + e.getMessage());
    }
  }

  public static void requestOpenGpx() {
    if (instance == null) {
      nativeOnGpxOpened(null, null, "MainActivity indisponible");
      return;
    }

    instance.runOnUiThread(() -> {
      Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
      intent.addCategory(Intent.CATEGORY_OPENABLE);
      intent.setType("*/*");
      intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {
          "application/gpx+xml",
          "application/xml",
          "text/xml",
          "*/*",
      });
      instance.startActivityForResult(intent, REQUEST_OPEN_GPX);
    });
  }

  public static void requestSaveGpx(String suggestedName, byte[] data) {
    if (instance == null) {
      nativeOnGpxSaved(null, "MainActivity indisponible");
      return;
    }

    pendingSaveData = data;
    instance.runOnUiThread(() -> {
      Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
      intent.addCategory(Intent.CATEGORY_OPENABLE);
      intent.setType("application/gpx+xml");
      intent.putExtra(Intent.EXTRA_TITLE, suggestedName);
      instance.startActivityForResult(intent, REQUEST_SAVE_GPX);
    });
  }

  @Override
  protected void onCreate(Bundle savedInstanceState) {
      super.onCreate(savedInstanceState);
      instance = this;

      // Keep IME insets propagation intact for GameActivity text input handling.
      View content = getWindow().getDecorView().findViewById(android.R.id.content);
      ViewCompat.setOnApplyWindowInsetsListener(content, (v, windowInsets) -> {
        Insets insets = windowInsets.getInsets(WindowInsetsCompat.Type.systemBars());

        ViewGroup.MarginLayoutParams mlp = (ViewGroup.MarginLayoutParams) v.getLayoutParams();
        mlp.topMargin = insets.top;
        mlp.leftMargin = insets.left;
        mlp.bottomMargin = insets.bottom;
        mlp.rightMargin = insets.right;
        v.setLayoutParams(mlp);

        return windowInsets;
      });

      WindowCompat.setDecorFitsSystemWindows(getWindow(), true);
      content.setFocusable(true);
      content.setFocusableInTouchMode(true);
      content.requestFocus();
  }

      @Override
      protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);

        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
          if (requestCode == REQUEST_OPEN_GPX) {
            nativeOnGpxOpened(null, null, "Selection de fichier annulee");
          } else if (requestCode == REQUEST_SAVE_GPX) {
            nativeOnGpxSaved(null, "Sauvegarde annulee");
          }
          return;
        }

        Uri uri = data.getData();

        if (requestCode == REQUEST_OPEN_GPX) {
          handleOpenResult(uri);
        } else if (requestCode == REQUEST_SAVE_GPX) {
          handleSaveResult(uri);
        }
      }

      @Override
      public void onRequestPermissionsResult(int requestCode, @NonNull String[] permissions, @NonNull int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);

        if (requestCode == REQUEST_BACKGROUND_LOCATION_PERMISSION) {
          if (!hasBackgroundLocationPermission()) {
            publishLocation(Double.NaN, Double.NaN, "Permission GPS arriere-plan refusee");
          }
          return;
        }

        if (requestCode != REQUEST_LOCATION_PERMISSION) {
          return;
        }

        boolean granted = false;
        for (int result : grantResults) {
          if (result == PackageManager.PERMISSION_GRANTED) {
            granted = true;
            break;
          }
        }

        if (granted) {
          fetchLocation();
          requestBackgroundLocationPermissionIfNeeded();
        } else {
          nativeOnLocationUpdated(Double.NaN, Double.NaN, "Permission GPS refusee");
        }
      }

      private void handleOpenResult(Uri uri) {
        try {
          String fileName = queryDisplayName(uri);
          if (fileName == null || fileName.isEmpty()) {
            fileName = "fichier.gpx";
          }

          byte[] bytes = readAllBytes(uri);
          nativeOnGpxOpened(fileName, bytes, null);
        } catch (Exception e) {
          nativeOnGpxOpened(null, null, "Erreur ouverture GPX: " + e.getMessage());
        }
      }

      private void handleSaveResult(Uri uri) {
        if (pendingSaveData == null) {
          nativeOnGpxSaved(null, "Aucune donnee a sauvegarder");
          return;
        }

        try {
          writeAllBytes(uri, pendingSaveData);
          pendingSaveData = null;

          String fileName = queryDisplayName(uri);
          if (fileName == null || fileName.isEmpty()) {
            fileName = "fichier.gpx";
          }
          nativeOnGpxSaved(fileName, null);
        } catch (Exception e) {
          nativeOnGpxSaved(null, "Erreur sauvegarde GPX: " + e.getMessage());
        }
      }

      private byte[] readAllBytes(Uri uri) throws Exception {
        try (InputStream input = getContentResolver().openInputStream(uri);
           ByteArrayOutputStream output = new ByteArrayOutputStream()) {
          if (input == null) {
            throw new Exception("Impossible d'ouvrir le flux de lecture");
          }

          byte[] buffer = new byte[8192];
          int read;
          while ((read = input.read(buffer)) != -1) {
            output.write(buffer, 0, read);
          }
          return output.toByteArray();
        }
      }

      private void writeAllBytes(Uri uri, byte[] bytes) throws Exception {
        try (OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
          if (output == null) {
            throw new Exception("Impossible d'ouvrir le flux d'ecriture");
          }
          output.write(bytes);
          output.flush();
        }
      }

      private String queryDisplayName(Uri uri) {
        Cursor cursor = null;
        try {
          cursor = getContentResolver().query(uri, null, null, null, null);
          if (cursor != null && cursor.moveToFirst()) {
            int nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
            if (nameIndex >= 0) {
              return cursor.getString(nameIndex);
            }
          }
        } catch (Exception ignored) {
        } finally {
          if (cursor != null) {
            cursor.close();
          }
        }
        return null;
      }

  @Override
  public boolean onTouchEvent(MotionEvent event) {
      // Offset the location so it fits the view with margins caused by insets.

      int[] location = new int[2];
      findViewById(android.R.id.content).getLocationOnScreen(location);
      event.offsetLocation(-location[0], -location[1]);
      return super.onTouchEvent(event);
  }

      @Override
      protected void onPause() {
        super.onPause();
        setAppInBackground(true);
        if (locationUpdatesActive) {
          LocationForegroundService.start(this);
        }
      }

      @Override
      protected void onResume() {
        super.onResume();
        setAppInBackground(false);
        LocationForegroundService.stop(this);
      }
}
