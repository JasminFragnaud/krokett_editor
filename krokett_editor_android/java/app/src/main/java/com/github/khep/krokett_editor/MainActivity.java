package com.github.khep.krokett_editor;

import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.provider.OpenableColumns;

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

  private static MainActivity instance;
  private static byte[] pendingSaveData;

  static {
    System.loadLibrary("main");
  }

  private static native void setAppInBackground(boolean isBackground);
  private static native void nativeOnGpxOpened(String name, byte[] data, String error);
  private static native void nativeOnGpxSaved(String fileName, String error);

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
      instance = this;

      // Shrink view so it does not get covered by insets.

      View content = getWindow().getDecorView().findViewById(android.R.id.content);
      ViewCompat.setOnApplyWindowInsetsListener(content, (v, windowInsets) -> {
        Insets insets = windowInsets.getInsets(WindowInsetsCompat.Type.systemBars());

        ViewGroup.MarginLayoutParams mlp = (ViewGroup.MarginLayoutParams) v.getLayoutParams();
        mlp.topMargin = insets.top;
        mlp.leftMargin = insets.left;
        mlp.bottomMargin = insets.bottom;
        mlp.rightMargin = insets.right;
        v.setLayoutParams(mlp);

        return WindowInsetsCompat.CONSUMED;
      });

      WindowCompat.setDecorFitsSystemWindows(getWindow(), true);

      super.onCreate(savedInstanceState);
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
      }

      @Override
      protected void onResume() {
        super.onResume();
        setAppInBackground(false);
      }
}
