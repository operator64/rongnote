import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';

// libsodium-wrappers-sumo's ESM build imports `./libsodium-sumo.mjs`, but
// that file lives in the separate `libsodium-sumo` package — upstream
// packaging bug. Rewrite the broken relative specifier so the resolver
// looks in the right package.
const fixLibsodiumImport: Plugin = {
  name: 'fix-libsodium-relative-import',
  enforce: 'pre',
  async resolveId(source, importer) {
    if (
      source === './libsodium-sumo.mjs' &&
      importer?.includes('libsodium-wrappers-sumo')
    ) {
      const resolved = await this.resolve('libsodium-sumo', importer, {
        skipSelf: true
      });
      return resolved ?? null;
    }
    return null;
  }
};

export default defineConfig({
  plugins: [fixLibsodiumImport, sveltekit()],
  optimizeDeps: {
    // Pre-bundle the sumo package and its peer so esbuild applies the plugin
    // during dep optimization too.
    include: ['libsodium-wrappers-sumo', 'libsodium-sumo'],
    esbuildOptions: {
      target: 'es2022',
      plugins: [
        {
          name: 'fix-libsodium-relative-import-esbuild',
          setup(build) {
            build.onResolve(
              { filter: /^\.\/libsodium-sumo\.mjs$/ },
              (args) => {
                if (!args.importer.includes('libsodium-wrappers-sumo')) return;
                return build.resolve('libsodium-sumo', {
                  kind: 'import-statement',
                  resolveDir: args.resolveDir
                });
              }
            );
          }
        }
      ]
    }
  },
  build: {
    target: 'es2022'
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': { target: 'http://localhost:8080', changeOrigin: false },
      '/healthz': { target: 'http://localhost:8080', changeOrigin: false }
    }
  }
});
