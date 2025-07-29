Project Architecture

  Cargo Workspace Structure:
  - sanitiser/ - Core PDF sanitization library
  - web_server/ - HTTP API server for handling sanitization requests
  - cli/ - Command-line interface

  Web Server (web_server/)

  Main Components:
  - Entry Point: main.rs:8 - Initializes telemetry and starts the application
  - Application Setup: startup.rs:34 - Configures the Actix-web server with routes and services
  - API Endpoints:
    - GET /management/health - Health check endpoint
    - POST /sanitise/pdf - PDF upload and sanitization queueing endpoint (50MB max payload)

  Key Services:
  - File Storage (storage.rs:6): Simple file system storage with store/retrieve/delete operations
  - Job Scheduler (workers/job.rs:45): SQLite-backed background job processing using Apalis framework
  - Configuration (app_settings.rs:6): YAML-based config with environment variable overrides

  PDF Sanitization Process (sanitiser/)

  Core Sanitization Logic (sanitise.rs:55):
  1. Page Rasterization: Converts each PDF page to a high-resolution bitmap (300 DPI)
  2. Batch Processing: Processes pages in chunks of 5 to manage memory usage
  3. Image Regeneration: Converts bitmaps to PNG, then embeds as JPEG in new PDF (70% quality)
  4. PDF Assembly: Merges temporary PDF chunks into final sanitized document
  5. Cleanup: Removes temporary files after processing

  Security Approach:
  - Complete Regeneration: Creates entirely new PDF from page screenshots, eliminating embedded malicious content
  - Process Isolation: Uses [procspawn](https://docs.rs/procspawn/latest/procspawn/) to run PDFium in separate processes (avoiding thread-safety issues)
  - Memory Management: Reuses bitmap containers and processes pages in batches

  Key Libraries:
  - PDFium: For PDF parsing and rendering (via pdfium-render)
  - printpdf: For generating new PDF documents
  - Apalis: Background job processing with SQLite storage
  - Actix-web: HTTP framework with OpenTelemetry tracing

  Request Flow

  1. Client uploads PDF to /sanitise/pdf (routes/sanitise.rs:12)
  2. File stored with UUID filename (sanitise.rs:17)
  3. Job queued for background processing (job.rs:91)
  4. Worker spawns isolated process to regenerate PDF (job.rs:148)
  5. Original file deleted, sanitized version available

  The architecture prioritizes security through complete PDF regeneration at the cost of file size (can be 10x larger) and processing time, making it effective for
  sanitizing potentially malicious PDFs by eliminating embedded threats.
