package com.leishuo.galaxyrecorder

import java.io.File
import java.nio.file.Files
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class MediaFilesTest {
    @get:Rule val folder = TemporaryFolder()
    private val data get() = folder.root
    private val files get() = File(data,"files").apply { mkdirs() }
    private fun resolve(file:File)=MediaFiles.resolve(data,files,file.path)
    private fun refused(file:File) { try {resolve(file);fail("Must reject files outside media roots")} catch (_:IllegalArgumentException) {} }
    @Test fun tauriRecordingsAndNativeRecordingsRemainReadable() {
        for(root in listOf(File(data,"media"),File(files,"media"))) {
            root.mkdirs();val video=File(root,"saved.mp4").apply{writeText("fixture")}
            assertEquals(video.canonicalFile,resolve(video))
        }
    }
    @Test fun fileProviderLegacyRootMatchesTauriDataDirectory() {
        assertEquals(File(data,"media").canonicalFile,File(files,"../media").canonicalFile)
    }
    @Test fun settingsAndEscapingPathsStayPrivate() {
        refused(File(data,"settings.json"));refused(File(files,"media/../../settings.json"));refused(File(files,"diagnostics/crash.log"))
    }
    @Test fun symlinkToNonMediaFileIsRejected() {
        val secret=File(data,"secret").apply{writeText("private")};val media=File(data,"media").apply{mkdirs()}
        val link=File(media,"fake.mp4");Files.createSymbolicLink(link.toPath(),secret.toPath());refused(link)
    }
}
