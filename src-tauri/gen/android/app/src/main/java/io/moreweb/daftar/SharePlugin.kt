// The phone's own share sheet.
//
// The shopkeeper can send an invoice through his Telegram bot (which the app
// uploads to itself) or through whatever else is on his phone: WhatsApp, the SMS
// messenger, a printer app. Both sit side by side on the screen and he chooses.
//
// Nothing here talks to a server. It hands text, or one file, to an app he picks
// in Android's own sheet — that sheet is the only place the choice is made.
//
// Reached from the interface as:
//   invoke("plugin:share|share",     { text })
//   invoke("plugin:share|shareFile", { path, mime, text })
// The Rust half that registers this class is src-tauri/src/android_share.rs.

package io.moreweb.daftar

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.util.Log
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

/** The heading Android prints above the list of apps. */
private const val CHOOSER_TITLE = "فرستادن با…"

private const val LOG_TAG = "SharePlugin"

/** What the interface sends: a message, and optionally one file. */
@InvokeArg
class ShareArgs {
  /** The message itself. A missing text is refused, never a crash. */
  var text: String? = null

  /** Absolute path of a file inside the app's own files folder. */
  var path: String? = null

  /** What the file is. A PDF when he does not say otherwise. */
  var mime: String? = null

  /** A subject line, for the apps that show one (mail). Optional. */
  var subject: String? = null
}

@TauriPlugin
class SharePlugin(private val activity: Activity) : Plugin(activity) {

  /**
   * The FileProvider authority: the application id plus `.files`, which is what
   * `${applicationId}.files` becomes in AndroidManifest.xml. Read from the app
   * itself so the two can never drift apart and leave `getUriForFile` throwing.
   */
  private val authority: String
    get() = "${activity.packageName}.files"

  /** `plugin:share|share` — the text alone, as a ready-made message. */
  @Command
  fun share(invoke: Invoke) {
    try {
      val args: ShareArgs? = invoke.parseArgs(ShareArgs::class.java)
      val text = args?.text?.trim().orEmpty()
      if (text.isEmpty()) {
        // An empty sheet is worse than a sentence: he pressed send with nothing
        // in his hands, and the app should say so instead of opening a chooser.
        invoke.reject("متنی برای فرستادن نیست")
        return
      }

      val intent = Intent(Intent.ACTION_SEND)
      intent.type = "text/plain"
      intent.putExtra(Intent.EXTRA_TEXT, text)
      args?.subject?.trim()?.let { subject ->
        if (subject.isNotEmpty()) {
          intent.putExtra(Intent.EXTRA_SUBJECT, subject)
        }
      }
      launch(intent, invoke)
    } catch (ex: Exception) {
      fail(invoke, ex)
    }
  }

  /** `plugin:share|shareFile` — the PDF (or any file) plus its caption. */
  @Command
  fun shareFile(invoke: Invoke) {
    try {
      val args: ShareArgs? = invoke.parseArgs(ShareArgs::class.java)
      val path = args?.path?.trim().orEmpty()
      if (path.isEmpty()) {
        invoke.reject("فایلی برای فرستادن نیست")
        return
      }

      val file = File(path)
      if (!file.isFile) {
        invoke.reject("فایل پیدا نشد")
        return
      }

      // The other app must not receive a `file://` path — Android refuses those
      // since 7.0. It gets a `content://` URI that carries a temporary read
      // grant for exactly this one file.
      val uri = FileProvider.getUriForFile(activity, authority, file)
      val mime = args?.mime?.trim().orEmpty().ifEmpty { "application/pdf" }

      val intent = Intent(Intent.ACTION_SEND)
      intent.type = mime
      intent.putExtra(Intent.EXTRA_STREAM, uri)
      // Both of these are needed on some Android versions: the flag asks for
      // the grant, the clip data names the URI the grant is for.
      intent.clipData = ClipData.newRawUri(null, uri)
      intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)

      val caption = args?.text?.trim().orEmpty()
      if (caption.isNotEmpty()) {
        intent.putExtra(Intent.EXTRA_TEXT, caption)
      }
      val subject = args?.subject?.trim().orEmpty()
      if (subject.isNotEmpty()) {
        intent.putExtra(Intent.EXTRA_SUBJECT, subject)
      }

      launch(intent, invoke)
    } catch (ex: Exception) {
      fail(invoke, ex)
    }
  }

  /**
   * `plugin:share|installApk` — hands a downloaded APK to Android's own
   * installer.
   *
   * Android then asks him "install this app?" by itself. That dialog is the
   * point of doing it this way: the app never installs anything silently, and
   * if he has not yet allowed this app to install unknown apps, Android says so
   * and offers the switch. `REQUEST_INSTALL_PACKAGES` in AndroidManifest.xml is
   * what makes that dialog reachable at all.
   */
  @Command
  fun installApk(invoke: Invoke) {
    try {
      val args: ShareArgs? = invoke.parseArgs(ShareArgs::class.java)
      val path = args?.path?.trim().orEmpty()
      if (path.isEmpty()) {
        invoke.reject("فایلی برای نصب نیست")
        return
      }

      val file = File(path)
      if (!file.isFile) {
        invoke.reject("فایل نصب پیدا نشد")
        return
      }

      // The installer is another app: it reads the APK through a temporary
      // grant, never through a file path.
      val uri = FileProvider.getUriForFile(activity, authority, file)
      val intent = Intent(Intent.ACTION_VIEW)
      intent.setDataAndType(uri, "application/vnd.android.package-archive")
      intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      // The installer runs in its own task, so this one is required too.
      intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      open(intent, invoke)
    } catch (ex: Exception) {
      fail(invoke, ex)
    }
  }

  /** Opens the sheet on the UI thread, which is the only thread allowed to. */
  private fun launch(intent: Intent, invoke: Invoke) {
    val chooser = Intent.createChooser(intent, CHOOSER_TITLE)
    chooser.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    open(chooser, invoke)
  }

  /** Starts one intent on the UI thread. */
  private fun open(intent: Intent, invoke: Invoke) {
    activity.runOnUiThread {
      try {
        activity.startActivity(intent)
        invoke.resolve()
      } catch (ex: Exception) {
        fail(invoke, ex)
      }
    }
  }

  /**
   * Logs what actually went wrong for whoever is debugging the phone, and tells
   * the shopkeeper one short sentence in his own language.
   */
  private fun fail(invoke: Invoke, ex: Exception) {
    Log.e(LOG_TAG, "share failed", ex)
    invoke.reject("فرستادن نشد")
  }
}
