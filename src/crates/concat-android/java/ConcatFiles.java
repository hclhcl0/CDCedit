// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer Abdullah and Concat contributors

package app.cdcedit.editor;

import android.Manifest;
import android.app.Activity;
import android.app.Fragment;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.util.Log;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * The system's document picker, for an activity that has no Java of its
 * own to receive its answer.
 *
 * The window is a NativeActivity: the framework's own class, with nothing
 * of ours to override, and a picker's result comes back only through an
 * activity's or a fragment's onActivityResult. So this is a fragment - a
 * headless one, added to the activity for the length of one pick - and
 * the result comes back here.
 *
 * What is picked is a content URI, which the engine's decoder cannot open:
 * it reads files by path. Each is copied into the app's own external
 * files, under Imported/, keeping its display name, and the paths are
 * what the engine is handed - through the native method below, which the
 * Rust side registers before it asks for a pick.
 *
 * The way out is here as well: a finished export is written into the
 * app's own folder, which on a phone nothing but the app can open, so
 * {@link #publishVideo} moves it into the phone's Movies through the
 * media store, where the gallery and every file manager show it.
 *
 * And the microphone, for a voiceover: a permission granted while the
 * app runs, asked for through the activity by {@link #askMicrophone}.
 *
 * Compiled by build.rs with the SDK's javac and d8, and loaded at run time
 * from the dex inside the binary; see src/lib.rs.
 */
public class ConcatFiles extends Fragment {
    private static final int REQUEST_PICK = 0xC0C4;
    private static final int REQUEST_MICROPHONE = 0xC0C5;
    private static final int REQUEST_STORAGE = 0xC0C6;
    private static final int REQUEST_CAPCUT_TREE = 0xC0C7;
    private static final String TAG = "concat-files";

    /**
     * Where a finished export goes, under the phone's storage: Movies/CDCedit.
     * Null before Android 10, which has no media store an app can write
     * through without a permission; there the file stays in the app's folder.
     */
    public static String publishedFolder() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            return null;
        }
        return Environment.DIRECTORY_MOVIES + "/CDCedit";
    }

    /**
     * Moves the video at {@code path} into {@link #publishedFolder()} through
     * the media store and deletes the original. Two strings come back: the
     * file's path under the phone's storage and null, or null and why it
     * could not be moved - in which case the original is still where it was.
     * Blocking: a video is big and the copy takes a while, so this is called
     * off the window's thread.
     */
    public static String[] publishVideo(Activity activity, String path) {
        String folder = publishedFolder();
        if (folder == null) {
            return new String[] {null, "this phone has no media store to put it in"};
        }
        String mime = "video/mp4";
        String lower = path.toLowerCase();
        if (lower.endsWith(".mkv")) {
            mime = "video/x-matroska";
        } else if (lower.endsWith(".webm")) {
            mime = "video/webm";
        } else if (lower.endsWith(".mov")) {
            mime = "video/quicktime";
        } else if (lower.endsWith(".3gp")) {
            mime = "video/3gpp";
        }
        return publish(activity, path, mime, folder,
                MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY));
    }

    /**
     * {@link #publishVideo} for a small file that is not a video - a
     * subtitle file - of MIME type {@code mime}: moved into the phone's
     * Download/CDCedit, which every file manager shows, under its own name
     * or "name (1)" where that is taken. The same two strings come back.
     */
    public static String[] publishDocument(Activity activity, String path, String mime) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            return new String[] {null, "this phone has no media store to put it in"};
        }
        return publish(activity, path, mime, Environment.DIRECTORY_DOWNLOADS + "/CDCedit",
                MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY));
    }

    /** Moves the file at {@code path} into {@code collection}, under {@code folder}. */
    private static String[] publish(
            Activity activity, String path, String mime, String folder, Uri collection) {
        File file = new File(path);
        ContentResolver resolver = activity.getContentResolver();
        ContentValues values = new ContentValues();
        values.put(MediaStore.MediaColumns.DISPLAY_NAME, file.getName());
        values.put(MediaStore.MediaColumns.MIME_TYPE, mime);
        values.put(MediaStore.MediaColumns.RELATIVE_PATH, folder);
        values.put(MediaStore.MediaColumns.IS_PENDING, 1);
        Uri uri;
        try {
            uri = resolver.insert(collection, values);
        } catch (Exception e) {
            return new String[] {null, "the media store refused the file: " + e.getMessage()};
        }
        if (uri == null) {
            return new String[] {null, "the media store refused the file"};
        }
        InputStream in = null;
        OutputStream out = null;
        try {
            in = new FileInputStream(file);
            out = resolver.openOutputStream(uri);
            if (out == null) {
                throw new IOException("the media store would not open the file for writing");
            }
            byte[] buffer = new byte[1 << 16];
            int read;
            while ((read = in.read(buffer)) > 0) {
                out.write(buffer, 0, read);
            }
            out.close();
            out = null;
            values.clear();
            values.put(MediaStore.MediaColumns.IS_PENDING, 0);
            resolver.update(uri, values, null, null);
        } catch (Exception e) {
            try {
                resolver.delete(uri, null, null);
            } catch (Exception ignored) {
            }
            return new String[] {null, e.getMessage() == null ? e.toString() : e.getMessage()};
        } finally {
            try {
                if (in != null) in.close();
            } catch (IOException ignored) {
            }
            try {
                if (out != null) out.close();
            } catch (IOException ignored) {
            }
        }
        file.delete();
        String relName = displayName(activity, uri);
        try {
            File publicFile = new File(Environment.getExternalStorageDirectory(), folder + "/" + relName);
            android.media.MediaScannerConnection.scanFile(
                activity.getApplicationContext(),
                new String[] { publicFile.getAbsolutePath() },
                new String[] { mime },
                null
            );
        } catch (Exception ignored) {
        }
        // The store may have renamed it: a second "name.mp4" is "name (1).mp4".
        return new String[] {folder + "/" + relName, null};
    }

    /**
     * Whether the app may record from the microphone. When it may not yet,
     * the system's question is put up and false comes back at once; the
     * answer is read the next time this is asked, which is the voiceover
     * panel's Try again. From any thread.
     */
    public static boolean askMicrophone(final Activity activity) {
        if (Build.VERSION.SDK_INT < 23) {
            return true;
        }
        if (activity.checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                == PackageManager.PERMISSION_GRANTED) {
            return true;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                activity.requestPermissions(
                        new String[] {Manifest.permission.RECORD_AUDIO}, REQUEST_MICROPHONE);
            }
        });
        return false;
    }

    /**
     * Checks and requests storage permissions on Android.
     */
    public static boolean askStorage(final Activity activity) {
        if (activity == null) return true;
        if (Build.VERSION.SDK_INT >= 30) {
            // Android 11+ (API 30+): Manage External Storage
            if (Environment.isExternalStorageManager()) {
                return true;
            }
            try {
                Intent intent = new Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION);
                intent.setData(Uri.parse("package:" + activity.getPackageName()));
                activity.startActivity(intent);
            } catch (Exception e) {
                try {
                    Intent intent = new Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION);
                    activity.startActivity(intent);
                } catch (Exception ignored) {}
            }
            return false;
        } else if (Build.VERSION.SDK_INT >= 23) {
            // Android 6 - 10: Standard storage permissions
            if (activity.checkSelfPermission(Manifest.permission.READ_EXTERNAL_STORAGE)
                    == PackageManager.PERMISSION_GRANTED) {
                return true;
            }
            activity.runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    activity.requestPermissions(
                            new String[] {
                                Manifest.permission.READ_EXTERNAL_STORAGE,
                                Manifest.permission.WRITE_EXTERNAL_STORAGE
                            }, REQUEST_STORAGE);
                }
            });
            return false;
        }
        return true;
    }

    /**
     * Extracts bundled assets (ytdlp, python.zip) to targetDir if not already extracted.
     */
    public static void extractAssets(Activity activity, String targetDir) {
        if (activity == null || targetDir == null) return;
        File dir = new File(targetDir);
        dir.mkdirs();
        String[] files = new String[] {"ytdlp", "python.zip", "ffmpeg.zip"};
        for (String name : files) {
            File dest = new File(dir, name);
            if (dest.exists() && dest.length() > 0) continue;
            try (InputStream in = activity.getAssets().open(name);
                 OutputStream out = new FileOutputStream(dest)) {
                byte[] buf = new byte[65536];
                int read;
                while ((read = in.read(buf)) > 0) {
                    out.write(buf, 0, read);
                }
            } catch (Exception e) {
                android.util.Log.w("ConcatFiles", "Could not extract " + name + ": " + e.getMessage());
            }
        }
    }

    /**
     * Returns the app's native library directory where embedded binaries
     * (libpython.so, libffmpeg.so, libqjs.so, libytdlp.so) live with execute permissions.
     */
    public static String nativeLibraryDir(Activity activity) {
        if (activity == null || activity.getApplicationInfo() == null) {
            return null;
        }
        return activity.getApplicationInfo().nativeLibraryDir;
    }

    /**
     * Opens a web address in the browser, or in whichever app claims it -
     * the launch screen's guides, the repository, the Discord invite. Null
     * when it went, or why it could not. Asked straight rather than through
     * resolveActivity, which from Android 11 answers no for any app the
     * manifest does not name in its queries. From any thread.
     */
    public static String openLink(Activity activity, String url) {
        try {
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity.startActivity(intent);
            return null;
        } catch (android.content.ActivityNotFoundException e) {
            return "no app on this phone opens " + url;
        } catch (RuntimeException e) {
            Log.w(TAG, "could not open " + url, e);
            return String.valueOf(e.getMessage());
        }
    }

    /**
     * Reads text from the Android system clipboard.
     */
    public static String getClipboardText(final Activity activity) {
        if (activity == null) return null;
        try {
            final String[] result = new String[] {null};
            final java.util.concurrent.CountDownLatch latch = new java.util.concurrent.CountDownLatch(1);
            activity.runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    try {
                        android.content.ClipboardManager cm =
                            (android.content.ClipboardManager) activity.getSystemService(android.content.Context.CLIPBOARD_SERVICE);
                        if (cm != null && cm.hasPrimaryClip()) {
                            ClipData clip = cm.getPrimaryClip();
                            if (clip != null && clip.getItemCount() > 0) {
                                CharSequence text = clip.getItemAt(0).coerceToText(activity);
                                if (text != null && text.length() > 0) {
                                    result[0] = text.toString();
                                }
                            }
                        }
                    } catch (Exception e) {
                        Log.w(TAG, "getClipboardText error: " + e.getMessage());
                    } finally {
                        latch.countDown();
                    }
                }
            });
            latch.await(300, java.util.concurrent.TimeUnit.MILLISECONDS);
            return result[0];
        } catch (Exception e) {
            return null;
        }
    }

    /** Registered from Rust: the picked files' paths, or none. */
    public static native void filesPicked(String[] paths);

    /** Registered from Rust: reports when CapCut tree sync finishes with the number of drafts found. */
    public static native void capcutTreeSynced(int count);

    /**
     * Shows the picker, offering the MIME types in {@code types} - comma
     * separated, "video/*,audio/*" - or every file when it is empty. From
     * any thread; the fragment is added on the UI thread.
     */
    public static void pick(final Activity activity, final String types) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                ConcatFiles fragment = new ConcatFiles();
                activity.getFragmentManager()
                        .beginTransaction()
                        .add(fragment, TAG)
                        .commitAllowingStateLoss();
                activity.getFragmentManager().executePendingTransactions();

                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType("*/*");
                if (types != null && !types.isEmpty()) {
                    intent.putExtra(Intent.EXTRA_MIME_TYPES, types.split(","));
                }
                intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
                try {
                    fragment.startActivityForResult(intent, REQUEST_PICK);
                } catch (Exception e) {
                    fragment.done(new ArrayList<Uri>());
                }
            }
        });
    }

    @Override
    public void onActivityResult(int request, int result, Intent data) {
        if (request == REQUEST_CAPCUT_TREE) {
            final Activity activity = getActivity();
            try {
                getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
            } catch (Exception ignored) {}
            if (result == Activity.RESULT_OK && data != null && data.getData() != null && activity != null) {
                final Uri treeUri = data.getData();
                try {
                    int flags = data.getFlags() & (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
                    activity.getContentResolver().takePersistableUriPermission(treeUri, flags);
                    SharedPreferences prefs = activity.getSharedPreferences("concat_capcut", Context.MODE_PRIVATE);
                    prefs.edit().putString("capcut_tree_uri", treeUri.toString()).apply();
                } catch (Exception e) {
                    Log.w(TAG, "takePersistableUriPermission: " + e.getMessage());
                }
                new Thread(new Runnable() {
                    @Override
                    public void run() {
                        int synced = syncTreeDrafts(activity, treeUri);
                        capcutTreeSynced(synced);
                    }
                }, "capcut-tree-sync").start();
            } else {
                capcutTreeSynced(0);
            }
            return;
        }
        if (request != REQUEST_PICK) {
            return;
        }
        List<Uri> uris = new ArrayList<Uri>();
        if (result == Activity.RESULT_OK && data != null) {
            ClipData clip = data.getClipData();
            if (clip != null) {
                for (int i = 0; i < clip.getItemCount(); i++) {
                    Uri uri = clip.getItemAt(i).getUri();
                    if (uri != null) {
                        uris.add(uri);
                    }
                }
            } else if (data.getData() != null) {
                uris.add(data.getData());
            }
        }
        done(uris);
    }

    /** Copies what was picked and reports it; then the fragment goes. */
    private void done(final List<Uri> uris) {
        final Activity activity = getActivity();
        try {
            getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
        } catch (Exception ignored) {
        }
        if (activity == null || uris.isEmpty()) {
            filesPicked(new String[0]);
            return;
        }
        // Off the UI thread: a video is big and the copy takes a while.
        new Thread(new Runnable() {
            @Override
            public void run() {
                List<String> paths = new ArrayList<String>();
                File dir = new File(activity.getExternalFilesDir(null), "Imported");
                dir.mkdirs();
                for (Uri uri : uris) {
                    File out = unique(dir, displayName(activity, uri));
                    if (copy(activity, uri, out)) {
                        paths.add(out.getAbsolutePath());
                    }
                }
                filesPicked(paths.toArray(new String[0]));
            }
        }, TAG).start();
    }

    /** The name the picker showed for the file, or one made from the URI. */
    private static String displayName(Activity activity, Uri uri) {
        String name = null;
        Cursor cursor = null;
        try {
            cursor = activity.getContentResolver().query(uri, null, null, null, null);
            if (cursor != null && cursor.moveToFirst()) {
                int column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (column >= 0) {
                    name = cursor.getString(column);
                }
            }
        } catch (Exception ignored) {
        } finally {
            if (cursor != null) {
                cursor.close();
            }
        }
        if (name == null || name.isEmpty()) {
            name = uri.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "import";
        }
        // Whatever the provider called it, it is one file name.
        return name.replace('/', '_').replace('\\', '_');
    }

    /** `name`, or `name (2)`, `name (3)`... until nothing is overwritten. */
    private static File unique(File dir, String name) {
        File out = new File(dir, name);
        if (!out.exists()) {
            return out;
        }
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String ext = dot > 0 ? name.substring(dot) : "";
        for (int n = 2; ; n++) {
            out = new File(dir, stem + " (" + n + ")" + ext);
            if (!out.exists()) {
                return out;
            }
        }
    }

    private static boolean copy(Activity activity, Uri uri, File out) {
        InputStream in = null;
        OutputStream os = null;
        try {
            in = activity.getContentResolver().openInputStream(uri);
            if (in == null) {
                return false;
            }
            os = new FileOutputStream(out);
            byte[] buffer = new byte[1 << 16];
            int read;
            while ((read = in.read(buffer)) > 0) {
                os.write(buffer, 0, read);
            }
            return true;
        } catch (IOException | RuntimeException e) {
            // A provider that revokes its grant mid-copy throws a
            // SecurityException, not an IOException; uncaught on this
            // thread it closed the app and lost the rest of the pick.
            Log.w(TAG, "could not copy " + uri + ": " + e);
            out.delete();
            return false;
        } finally {
            try {
                if (in != null) in.close();
            } catch (IOException ignored) {
            }
            try {
                if (os != null) os.close();
            } catch (IOException ignored) {
            }
        }
    }

    /**
     * Prompts the user to pick/grant access to CapCut's data folder using Storage Access Framework (SAF).
     */
    public static void pickCapCutFolder(final Activity activity) {
        if (activity == null) return;
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                ConcatFiles fragment = new ConcatFiles();
                activity.getFragmentManager()
                        .beginTransaction()
                        .add(fragment, TAG)
                        .commitAllowingStateLoss();
                activity.getFragmentManager().executePendingTransactions();

                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    Uri initialUri = Uri.parse("content://com.android.externalstorage.documents/document/primary%3AAndroid%2Fdata%2Fcom.lemon.lvoverseas%2Ffiles");
                    intent.putExtra("android.provider.extra.INITIAL_URI", initialUri);
                }
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
                try {
                    fragment.startActivityForResult(intent, REQUEST_CAPCUT_TREE);
                } catch (Exception e) {
                    Log.w(TAG, "pickCapCutFolder error: " + e.getMessage());
                }
            }
        });
    }

    /**
     * Synchronizes any previously granted CapCut tree folder in the background.
     */
    public static void syncSavedCapCutTree(final Activity activity) {
        if (activity == null) return;
        SharedPreferences prefs = activity.getSharedPreferences("concat_capcut", Context.MODE_PRIVATE);
        String saved = prefs.getString("capcut_tree_uri", null);
        if (saved == null || saved.isEmpty()) return;
        final Uri uri = Uri.parse(saved);
        new Thread(new Runnable() {
            @Override
            public void run() {
                int synced = syncTreeDrafts(activity, uri);
                if (synced > 0) {
                    capcutTreeSynced(synced);
                }
            }
        }, "capcut-saved-tree-sync").start();
    }

    private static class DocItem {
        final String docId;
        final String name;
        final boolean isDir;
        DocItem(String docId, String name, boolean isDir) {
            this.docId = docId;
            this.name = name;
            this.isDir = isDir;
        }
    }

    private static List<DocItem> listDocChildren(ContentResolver cr, Uri treeUri, String parentDocId) {
        List<DocItem> list = new ArrayList<DocItem>();
        Cursor cursor = null;
        try {
            Uri childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId);
            cursor = cr.query(childrenUri, new String[] {
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE
            }, null, null, null);
            if (cursor != null) {
                while (cursor.moveToNext()) {
                    String docId = cursor.getString(0);
                    String name = cursor.getString(1);
                    String mime = cursor.getString(2);
                    boolean isDir = DocumentsContract.Document.MIME_TYPE_DIR.equals(mime);
                    list.add(new DocItem(docId, name, isDir));
                }
            }
        } catch (Exception ignored) {
        } finally {
            if (cursor != null) cursor.close();
        }
        return list;
    }

    private static void copyDraftFiles(ContentResolver cr, Uri treeUri, List<DocItem> files, File targetDir) {
        targetDir.mkdirs();
        for (DocItem f : files) {
            String lower = f.name.toLowerCase();
            if (lower.endsWith(".json") || lower.endsWith(".jpg") || lower.endsWith(".jpeg") || lower.endsWith(".png")) {
                File dest = new File(targetDir, f.name);
                if (!dest.exists() || dest.length() == 0) {
                    try {
                        Uri docUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, f.docId);
                        InputStream in = cr.openInputStream(docUri);
                        if (in != null) {
                            FileOutputStream out = new FileOutputStream(dest);
                            byte[] buffer = new byte[8192];
                            int read;
                            while ((read = in.read(buffer)) != -1) {
                                out.write(buffer, 0, read);
                            }
                            out.close();
                            in.close();
                        }
                    } catch (Exception ignored) {}
                }
            }
        }
    }

    public static int syncTreeDrafts(Activity activity, Uri treeUri) {
        if (activity == null || treeUri == null) return 0;
        int count = 0;
        try {
            ContentResolver cr = activity.getContentResolver();
            String rootDocId = DocumentsContract.getTreeDocumentId(treeUri);

            File destBase = new File(activity.getExternalFilesDir(null), "CapCut");
            destBase.mkdirs();

            List<DocItem> rootChildren = listDocChildren(cr, treeUri, rootDocId);
            boolean isDirectDraft = false;
            for (DocItem item : rootChildren) {
                if (item.name.equals("draft_content.json") || item.name.equals("draft_info.json")) {
                    isDirectDraft = true;
                    break;
                }
            }

            if (isDirectDraft) {
                File targetDir = new File(destBase, "Imported_CapCut_Draft");
                copyDraftFiles(cr, treeUri, rootChildren, targetDir);
                return 1;
            }

            DocItem draftsFolder = null;
            for (DocItem item : rootChildren) {
                if (item.isDir && (item.name.equalsIgnoreCase("newdrafts") || item.name.equalsIgnoreCase("drafts") || item.name.equalsIgnoreCase("draft"))) {
                    draftsFolder = item;
                    break;
                }
            }

            List<DocItem> draftFolders = (draftsFolder != null)
                ? listDocChildren(cr, treeUri, draftsFolder.docId)
                : rootChildren;

            for (DocItem draftDir : draftFolders) {
                if (!draftDir.isDir) continue;
                List<DocItem> draftFiles = listDocChildren(cr, treeUri, draftDir.docId);
                boolean isDraft = false;
                for (DocItem f : draftFiles) {
                    if (f.name.equals("draft_content.json") || f.name.equals("draft_info.json") || f.name.equals("draft_meta_info.json") || f.name.equals("template_content.json")) {
                        isDraft = true;
                        break;
                    }
                }
                if (isDraft) {
                    File targetDraftDir = new File(destBase, draftDir.name);
                    copyDraftFiles(cr, treeUri, draftFiles, targetDraftDir);
                    count++;
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "syncTreeDrafts error: " + e.getMessage());
        }
        return count;
    }
}
