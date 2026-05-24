// AmberHTML Kotlin/JVM binding (UniFFI + JNA).
//
// The Kotlin wrapper (src/main/kotlin/uniffi/amber_core/amber_core.kt) and the
// bundled native library (src/main/resources/<jna-platform>/libamber_core.*)
// are produced by generate.sh — run it before `./gradlew build`.
plugins {
    kotlin("jvm") version "1.9.23"
    `java-library`
    `maven-publish`
}

group = "io.github.afeique"
version = System.getenv("AMBER_VERSION") ?: "0.1.0"

repositories { mavenCentral() }

dependencies {
    // UniFFI's Kotlin backend calls the native library through JNA.
    implementation("net.java.dev.jna:jna:5.14.0")
    testImplementation(kotlin("test"))
}

tasks.test { useJUnitPlatform() }

kotlin { jvmToolchain(11) }

publishing {
    publications {
        create<MavenPublication>("maven") {
            artifactId = "amber-html"
            from(components["java"])
            pom {
                name.set("AmberHTML")
                description.set("Local-first web-page capture: Markdown, HTML, MHTML, WARC/WACZ, screenshot, PDF.")
                url.set("https://github.com/afeique/amber-html")
                licenses {
                    license { name.set("MIT OR Apache-2.0") }
                }
            }
        }
    }
}
