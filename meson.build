project('refdir', version: '0.2.0', license: 'MIT', meson_version: '>= 0.59')

id = 'net.xiupos.Refdir'
datadir = get_option('datadir')
manifest = meson.project_source_root() / 'Cargo.toml'

# Cargo builds the command and the app; Meson installs them
cargo = find_program('cargo')
custom_target(
  'cargo-build',
  output: ['refdir', 'refdir-gtk'],
  command: [
    cargo, 'build', '--release', '--manifest-path', manifest,
    '--target-dir', meson.project_build_root() / 'target',
    '&&', 'cp', 'target' / 'release' / 'refdir', 'target' / 'release' / 'refdir-gtk', '@OUTDIR@',
  ],
  build_always_stale: true,
  build_by_default: true,
  console: true,
  install: true,
  install_dir: get_option('bindir'),
)

# The full path: GNOME hides entries whose command is not in its PATH, which may lack ~/.local/bin
desktop = configure_file(
  input: 'data' / id + '.desktop.in',
  output: id + '.desktop',
  configuration: {'bindir': get_option('prefix') / get_option('bindir')},
  install_dir: datadir / 'applications',
)
install_data('data' / id + '.metainfo.xml', install_dir: datadir / 'metainfo')
# GNOME Shell's search: Shell reads the provider, and D-Bus starts the service it names
install_data('data' / id + '.search-provider.ini', install_dir: datadir / 'gnome-shell' / 'search-providers')
configure_file(
  input: 'data' / id + '.SearchProvider.service.in',
  output: id + '.SearchProvider.service',
  configuration: {'bindir': get_option('prefix') / get_option('bindir')},
  install_dir: datadir / 'dbus-1' / 'services',
)
install_data('data' / id + '.gschema.xml', install_dir: datadir / 'glib-2.0' / 'schemas')
install_subdir('data' / 'icons' / 'hicolor', install_dir: datadir / 'icons')
import('gnome').post_install(glib_compile_schemas: true, gtk_update_icon_cache: true, update_desktop_database: true)

test('cargo-test', cargo, args: ['test', '--manifest-path', manifest], timeout: 600)
desktop_file_validate = find_program('desktop-file-validate', required: false)
if desktop_file_validate.found()
  test('desktop-file', desktop_file_validate, args: desktop)
endif
appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test('metainfo', appstreamcli, args: ['validate', '--no-net', files('data' / id + '.metainfo.xml')])
endif
