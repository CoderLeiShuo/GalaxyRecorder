package com.leishuo.galaxyrecorder

import java.io.File

/** Both historical media roots belong to this app. Canonical paths also reject escaping symlinks. */
internal object MediaFiles {
    fun resolve(dataDir: File, filesDir: File, path: String): File {
        val file = File(path).canonicalFile
        val roots = listOf(File(dataDir, "media"), File(filesDir, "media"))
        require(roots.any { file.parentFile == it.canonicalFile }) { "不能读取媒体库之外的文件" }
        return file
    }
}
