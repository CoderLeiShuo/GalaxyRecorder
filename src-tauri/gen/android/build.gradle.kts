buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.3.1")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.2.10")
    }
}

allprojects {
    repositories {
        // Optional Google Maven mirror for networks that cannot reach dl.google.com.
        if (providers.environmentVariable("GALAXY_MAVEN_MIRROR").orNull == "1") {
            maven("https://maven.aliyun.com/repository/google") {
                content { includeGroupByRegex("androidx\\..*") }
            }
        }
        mavenCentral()
        google()
    }
}

tasks.register("clean").configure {
    delete("build")
}

