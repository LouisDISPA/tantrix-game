package dev.dioxus.main

import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.annotation.Keep
import androidx.core.content.FileProvider
import java.io.File

typealias BuildConfig = io.github.louisdispa.tantrix.BuildConfig

class MainActivity : WryActivity() {
    // Called from Rust on the activity thread. Keep the name through release shrinking.
    @Keep
    fun installUpdate(path: String): String = try {
        val apk = File(path)
        val flags = PackageManager.PackageInfoFlags.of(PackageManager.GET_SIGNING_CERTIFICATES.toLong())
        val candidate = requireNotNull(packageManager.getPackageArchiveInfo(apk.path, flags)) {
            "The downloaded APK is invalid."
        }
        val installed = packageManager.getPackageInfo(packageName, flags)
        require(candidate.packageName == packageName && candidate.longVersionCode > installed.longVersionCode) {
            "This APK is not a newer Tantrix release."
        }
        val signers = candidate.signingInfo?.apkContentsSigners
        require(!signers.isNullOrEmpty() && signers.all {
            packageManager.hasSigningCertificate(packageName, it.toByteArray(), PackageManager.CERT_INPUT_RAW_X509)
        }) { "The APK signing certificate does not match Tantrix." }

        if (!packageManager.canRequestPackageInstalls()) {
            startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
            "permission"
        } else {
            val uri = FileProvider.getUriForFile(this, "$packageName.updates", apk)
            startActivity(Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "application/vnd.android.package-archive")
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            })
            ""
        }
    } catch (error: Exception) {
        error.message ?: "Could not open the Android installer."
    }
}
