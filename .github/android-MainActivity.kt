package space.haex.vault

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
    // Reinstate ndk-context init that tao 0.34 used to do for us. Without
    // this, hickory-resolver (used by iroh for DNS) panics on first use
    // with "android context was not initialized". Implemented in Rust
    // (see Java_space_haex_vault_MainActivity_initializeNdkContext).
    private external fun initializeNdkContext()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        initializeNdkContext()
    }
}
