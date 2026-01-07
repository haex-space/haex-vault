export default defineNuxtPlugin({
  name: 'init-logger',
  enforce: 'pre',
  parallel: false,
  setup() {
    // Add global error handler for better debugging
    window.addEventListener('error', (event) => {
      console.error('[HaexSpace] Global error caught:', {
        message: event.message,
        filename: event.filename,
        lineno: event.lineno,
        colno: event.colno,
        error: event.error,
        stack: event.error?.stack,
      })
    })

    window.addEventListener('unhandledrejection', (event) => {
      // Provide more detailed error info including stack trace if available
      const errorInfo: Record<string, unknown> = {
        reason: event.reason,
        promise: event.promise,
      }

      // Try to extract more info from the rejection reason
      if (event.reason instanceof Error) {
        errorInfo.message = event.reason.message
        errorInfo.stack = event.reason.stack
        errorInfo.name = event.reason.name
      } else if (typeof event.reason === 'object' && event.reason !== null) {
        // Try to stringify the reason for better visibility
        try {
          errorInfo.reasonJson = JSON.stringify(event.reason)
        } catch {
          errorInfo.reasonType = typeof event.reason
        }
      }

      console.error('[HaexSpace] Unhandled rejection:', errorInfo)
    })
  },
})
