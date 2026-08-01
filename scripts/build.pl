#!/usr/bin/env perl
use strict;
use warnings;
use Cwd qw(abs_path getcwd);
use File::Basename qw(dirname);
use File::Copy qw(copy);
use File::Find qw(find);
use File::Path qw(make_path remove_tree);
use MIME::Base64 qw(decode_base64);
use POSIX qw(strftime);

my $script_dir = dirname(abs_path($0));
my $root_dir = abs_path("$script_dir/..");
my $core_root = "$root_dir/core";
my $config_file = "$root_dir/.config";
my $developer_root_public_key_file = "$root_dir/.pubkey";
my %build_options = (
    cached => 0,
);

for my $arg (@ARGV) {
    if ($arg eq '--cached') {
        $build_options{cached} = 1;
        next;
    }
    die "fatal: unknown build option: $arg\n";
}

sub dief {
    die "fatal: @_\n";
}

sub configure_developer_root_public_key {
    my ($path) = @_;
    return if defined($ENV{MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX})
        && $ENV{MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX} ne '';

    open my $fh, '<', $path or dief("open $path: $!");
    local $/;
    my $key = <$fh> // '';
    close $fh or dief("close $path: $!");
    $key =~ s/^\s+//;
    $key =~ s/\s+$//;
    dief("$path must contain exactly one 32-byte hexadecimal public key")
        if $key !~ /\A[0-9A-Fa-f]{64}\z/;
    $ENV{MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX} = lc $key;
}

configure_developer_root_public_key($developer_root_public_key_file);

sub run {
    my (@cmd) = @_;
    system @cmd;
    dief("command failed: @cmd") if $? != 0;
}

sub run_env {
    my ($env, @cmd) = @_;
    local %ENV = (%ENV, %{$env});
    run(@cmd);
}

sub cargo_env {
	my (%extra) = @_;
	return \%extra;
}

sub run_quiet {
    my (@cmd) = @_;
    open my $oldout, '>&', \*STDOUT or dief("dup stdout: $!");
    open STDOUT, '>', '/dev/null' or dief("redirect stdout: $!");
    system @cmd;
    my $rc = $?;
    open STDOUT, '>&', $oldout or dief("restore stdout: $!");
    close $oldout;
    dief("command failed: @cmd") if $rc != 0;
}

sub run_in_dir {
    my ($dir, @cmd) = @_;
    my $cwd = getcwd();
    chdir $dir or dief("chdir $dir: $!");
    run(@cmd);
    chdir $cwd or dief("chdir $cwd: $!");
}

sub capture_stdout {
    my (@cmd) = @_;
    open my $fh, '-|', @cmd or dief("spawn @cmd: $!");
    local $/;
    my $out = <$fh>;
    close $fh or dief("command failed: @cmd");
    return $out // '';
}

sub capture_stdout_env {
    my ($env, @cmd) = @_;
    local %ENV = (%ENV, %{$env});
    return capture_stdout(@cmd);
}

sub need_cmd {
    my ($cmd) = @_;
    system("command -v '$cmd' >/dev/null 2>&1");
    dief("required command not found: $cmd") if $? != 0;
}

sub need_file {
    my ($path) = @_;
    dief("required file not found: $path") if !-f $path;
}

sub need_dir {
    my ($path) = @_;
    dief("required directory not found: $path") if !-d $path;
}

sub install_file {
    my ($mode, $src, $dst) = @_;
    run('install', '-m', $mode, $src, $dst);
}

sub config_enabled {
    my ($value) = @_;
    return defined($value) && $value eq 'y';
}

sub config_to_01 {
    my ($value) = @_;
    return config_enabled($value) ? '1' : '0';
}

sub unquote_config_value {
    my ($value) = @_;

    $value =~ s/^\s+//;
    $value =~ s/\s+$//;

    if ($value =~ /^"(.*)"$/s) {
        $value = $1;
        $value =~ s/\\n/\n/g;
        $value =~ s/\\t/\t/g;
        $value =~ s/\\"/"/g;
        $value =~ s/\\\\/\\/g;
    }

    return $value;
}

sub read_config {
    my ($path) = @_;
    my %config;
    open my $fh, '<', $path or dief("open $path: $!");
    while (my $line = <$fh>) {
        chomp $line;
        next if $line =~ /^\s*#/ || $line =~ /^\s*$/;
        if ($line =~ /^\s*([A-Za-z_][A-Za-z0-9_]*)=(.*)$/) {
            $config{$1} = unquote_config_value($2);
            next;
        }
        dief("invalid config line in $path: $line");
    }
    close $fh;
    return %config;
}

sub capture_lines_env {
    my ($env, @cmd) = @_;
    local %ENV = (%ENV, %{$env});
    open my $fh, '-|', @cmd or dief("spawn @cmd: $!");
    my @lines;
    while (my $line = <$fh>) {
        chomp $line;
        push @lines, $line;
    }
    close $fh or dief("command failed: @cmd");
    return @lines;
}

sub write_build_info {
    my ($path, $root_dir) = @_;
    my $root_commit = `git -C '$root_dir' rev-parse HEAD 2>/dev/null`;
    chomp $root_commit;
    $root_commit = 'unknown' if $root_commit eq '';
    open my $fh, '>', $path or dief("open $path: $!");
    print {$fh} "build_number=", ($ENV{BUILD_NUMBER} // 'unassigned'), "\n";
    print {$fh} "manifest_commit=$root_commit\n";
    print {$fh} "github_sha=", ($ENV{GITHUB_SHA} // 'unknown'), "\n";
    print {$fh} "github_run_id=", ($ENV{GITHUB_RUN_ID} // 'unknown'), "\n";
    print {$fh} "built_at=", strftime('%Y-%m-%dT%H:%M:%SZ', gmtime), "\n";
    close $fh;
}

sub write_checksums {
    my ($artifact_dir, @files) = @_;
    my $cwd = getcwd();
    chdir $artifact_dir or dief("chdir $artifact_dir: $!");
    open my $oldout, '>&', \*STDOUT or dief("dup stdout: $!");
    open STDOUT, '>', 'SHA256SUMS' or dief("open SHA256SUMS: $!");
    system 'sha256sum', @files;
    my $rc = $?;
    open STDOUT, '>&', $oldout or dief("restore stdout: $!");
    close $oldout;
    chdir $cwd or dief("chdir $cwd: $!");
    dief("sha256sum failed") if $rc != 0;
}

sub append_checksum {
    my ($artifact_dir, @files) = @_;
    my $cwd = getcwd();
    chdir $artifact_dir or dief("chdir $artifact_dir: $!");
    open my $oldout, '>&', \*STDOUT or dief("dup stdout: $!");
    open STDOUT, '>>', 'SHA256SUMS' or dief("open SHA256SUMS: $!");
    system 'sha256sum', @files;
    my $rc = $?;
    open STDOUT, '>&', $oldout or dief("restore stdout: $!");
    close $oldout;
    chdir $cwd or dief("chdir $cwd: $!");
    dief("sha256sum append failed") if $rc != 0;
}

sub copy_tree {
    my ($src, $dst) = @_;
    remove_tree($dst);
    make_path($dst);
    run('cp', '-a', "$src/.", $dst);
}

sub cached_artifacts_current {
    my ($root, $stamp, $disk_image) = @_;
    return 0 if !$build_options{cached} || !-f $stamp || !-f $disk_image;

    my $stamp_mtime = (stat($stamp))[9] // 0;
    my $stale = 0;
    find(
        {
            no_chdir => 1,
            wanted => sub {
                return if $stale;
                my $path = $File::Find::name;
                my $relative = $path eq $root
                    ? ''
                    : substr($path, length($root) + 1);

                if (-d $path && ($relative eq 'out'
                    || $relative eq '.repo'
                    || $relative eq '.workspace'
                    || $relative =~ m{(?:^|/)\.git\z}
                    || $relative =~ m{(?:^|/)target\z}
                    || $relative =~ m{(?:^|/)node_modules\z}
                    || $relative eq 'libraries/libc/src'
                    || $relative eq 'libraries/libc/.generated'
                    || $relative eq 'libraries/fonts/out')) {
                    $File::Find::prune = 1;
                    return;
                }

                return if !-f $path;
                my $mtime = (stat($path))[9] // 0;
                $stale = 1 if $mtime > $stamp_mtime;
            },
        },
        $root,
    );
    return !$stale;
}

sub stage_font_assets {
    my ($fonts_src, $fonts_dst) = @_;
    need_dir($fonts_src);
    remove_tree($fonts_dst);
    make_path($fonts_dst);

    opendir my $dh, $fonts_src or dief("opendir $fonts_src: $!");
    for my $name (sort grep { $_ ne '.' && $_ ne '..' && $_ ne '.installed' } readdir $dh) {
        my $src = "$fonts_src/$name";
        my $dst = "$fonts_dst/$name";
        if (-d $src) {
            copy_tree($src, $dst);
        }
        elsif (-f $src) {
            install_file('0644', $src, $dst);
        }
        else {
            dief("unsupported font artifact: $src");
        }
    }
    closedir $dh;
}

sub stage_resource_tree {
    my ($resources_src, $rootfs_dst) = @_;
    need_dir($resources_src);
    find(
        {
            no_chdir => 1,
            preprocess => sub { sort @_ },
            wanted => sub {
                my $src = $File::Find::name;
                return if $src eq $resources_src;
                my $relative = substr($src, length($resources_src) + 1);
                my $dst = "$rootfs_dst/$relative";
                dief("symbolic links are not supported in resources: $src") if -l $src;
                if (-d $src) {
                    make_path($dst);
                }
                elsif (-f $src) {
                    install_file('0644', $src, $dst);
                }
                else {
                    dief("unsupported resource artifact: $src");
                }
            },
        },
        $resources_src,
    );
}

sub rewrite_cargo_paths {
    return;
}

sub latest_matching_file {
    my ($dir, $pattern) = @_;
    opendir my $dh, $dir or dief("opendir $dir: $!");
    my @files = sort grep {/$pattern/ && -f "$dir/$_"} readdir $dh;
    closedir $dh;
    dief("no matching file in $dir: $pattern") if !@files;
    return "$dir/$files[-1]";
}

sub build_newlib_runtime {
    my ($root_dir, $toolchain) = @_;
    my $user_root = "$root_dir/user";
    my $newlib_root = "$root_dir/libraries/newlib";
    my $bootstrap_target = 'x86_64-elf';
    my $final_target = 'x86_64-unknown-mochios';
    my $target_json = "$user_root/targets/$final_target.json";
    my $out_root = "$root_dir/out/newlib-port";
    my $newlib_build_dir = "$out_root/build-newlib";
    my $install_root = "$out_root/toolchain";
    my $sysroot_dir = "$install_root/$bootstrap_target";
    my $runtime_target_dir = "$out_root/cargo-target";
    my $hello_dir = "$out_root/hello";
    my $crt0_s = "$user_root/runtime/crt0.S";
    my $linker_script = "$user_root/runtime/linker.ld";
    my $hello_c = "$user_root/libc-port/tests/hello.c";

    for my $cmd (qw(cargo make nm readelf x86_64-elf-ar x86_64-elf-gcc x86_64-elf-ranlib)) {
        need_cmd($cmd);
    }
    need_dir("$root_dir/.repo");
    need_file("$newlib_root/configure");
    need_file("$user_root/Cargo.toml");
    need_file($target_json);
    need_file($crt0_s);
    need_file($linker_script);
    need_file($hello_c);

    make_path($out_root, $runtime_target_dir, $hello_dir);

    print "[test] user existing tests\n";
    run_env(
        cargo_env(),
        'cargo', "+$toolchain", 'test',
        '--manifest-path', "$user_root/Cargo.toml",
        '-p', 'mochi-user-syscall',
    );

    my $newlib_ready = -f "$sysroot_dir/lib/libc.a"
        && -f "$sysroot_dir/lib/libm.a"
        && -d "$sysroot_dir/include";
    if ($build_options{cached} && $newlib_ready) {
        print "[cache] reuse newlib toolchain\n";
    } else {
        remove_tree($newlib_build_dir, $install_root);
        make_path($newlib_build_dir, $install_root);

        print "[build] configure newlib\n";
        run_in_dir(
            $newlib_build_dir,
            'env',
            'CC_FOR_TARGET=x86_64-elf-gcc',
            'AR_FOR_TARGET=x86_64-elf-ar',
            'RANLIB_FOR_TARGET=x86_64-elf-ranlib',
            "$newlib_root/configure",
            "--target=$bootstrap_target",
            "--prefix=$install_root",
            '--disable-binutils',
            '--disable-gas',
            '--disable-gdb',
            '--disable-gprof',
            '--disable-libgloss',
            '--disable-multilib',
            '--disable-nls',
            '--disable-shared',
            '--disable-sim',
            '--disable-werror',
            '--disable-newlib-supplied-syscalls',
            '--enable-newlib-multithread=no',
            '--enable-newlib-retargetable-locking',
        );

        print "[build] newlib\n";
        my $jobs = capture_stdout('nproc');
        chomp $jobs;
        $jobs = 1 if $jobs !~ /^\d+$/ || $jobs < 1;
        run('make', '-C', $newlib_build_dir, "-j$jobs", 'all-target-newlib');

        print "[build] install newlib\n";
        run('make', '-C', $newlib_build_dir, 'install-target-newlib');
    }

    print "[build] mochiOS runtime\n";
    run_env(
        cargo_env(),
        'cargo', "+$toolchain", 'build',
        '-Z', 'json-target-spec',
        '-Z', 'build-std=core,compiler_builtins',
        '--manifest-path', "$user_root/Cargo.toml",
        '--package', 'mochi-user-newlib-runtime',
        '--release',
        '--target', $target_json,
        '--target-dir', $runtime_target_dir,
    );

    my $runtime_lib = "$runtime_target_dir/$final_target/release/libmochi_user_newlib_runtime.a";
    my $crt0_o = "$hello_dir/crt0.o";
    my $hello_o = "$hello_dir/hello.o";
    my $hello_elf = "$hello_dir/hello.elf";
    my $hello_map = "$hello_dir/hello.map";
    need_file($runtime_lib);

    print "[build] crt0\n";
    run('x86_64-elf-gcc', '-c', $crt0_s, '-o', $crt0_o);

    print "[build] hello.c\n";
    run(
        'x86_64-elf-gcc',
        "--sysroot=$sysroot_dir",
        '-isystem', "$sysroot_dir/include",
        '-ffreestanding',
        '-O2',
        '-c', $hello_c,
        '-o', $hello_o,
    );

    print "[link] hello.elf\n";
    run(
        'x86_64-elf-gcc',
        "--sysroot=$sysroot_dir",
        "-L$sysroot_dir/lib",
        '-static',
        '-nostdlib',
        '-nostartfiles',
        "-Wl,-T,$linker_script",
        '-Wl,-no-pie',
        '-Wl,-z,noexecstack',
        "-Wl,-Map,$hello_map",
        '-Wl,--start-group',
        $crt0_o,
        $hello_o,
        $runtime_lib,
        '-lc',
        '-lm',
        '-lgcc',
        '-Wl,--end-group',
        '-o', $hello_elf,
    );

    print "[check] hello.elf header\n";
    run('readelf', '-h', $hello_elf);
    run('readelf', '-l', $hello_elf);
    my $undefined = capture_stdout('nm', '-u', $hello_elf);
    dief('hello.elf still has undefined symbols') if $undefined =~ /\S/;
    print "[done] $hello_elf\n";
}

sub prepare_rust_sysroot_overlay {
    my ($root_dir, $toolchain, $out_root, $sysroot_overlay) = @_;
    my @overrides = qw(
        library/std/src/sys/pal/unix/mod.rs
        library/std/build.rs
        library/std/src/sys/args/unix.rs
        library/std/src/os/unix/mod.rs
        library/std/src/os/linux/mod.rs
        library/std/src/os/linux/fs.rs
        library/std/src/os/unix/fs.rs
        library/std/src/os/mod.rs
        library/std/src/sys/paths/unix.rs
        library/std/src/sys/random/mod.rs
        library/std/src/sys/fs/unix.rs
        library/std/src/sys/sync/mutex/mod.rs
        library/std/src/sys/sync/condvar/mod.rs
        library/std/src/sys/sync/thread_parking/mod.rs
        library/std/src/sys/sync/once/mod.rs
        library/std/src/sys/sync/rwlock/mod.rs
        library/std/src/sys/thread/mod.rs
        library/std/src/sys/thread_local/mod.rs
        library/proc_macro/Cargo.toml
        library/Cargo.toml
    );

    my $overlay_stamp = "$sysroot_overlay/.mochios-overlay-stamp";
    my $overlay_vendor = "$sysroot_overlay/lib/rustlib/src/rust/library/vendor";
    my $overlay_literal_escaper =
        "$sysroot_overlay/lib/rustlib/src/rust/vendor/rustc-literal-escaper/Cargo.toml";
    my $overlay_fresh = $build_options{cached}
        && -x "$sysroot_overlay/bin/rustc"
        && -d "$sysroot_overlay/lib/rustlib/src/rust"
        && -d $overlay_vendor
        && -f $overlay_literal_escaper
        && -f $overlay_stamp;
    if ($overlay_fresh) {
        my $stamp_mtime = (stat($overlay_stamp))[9] // 0;
        for my $rel (@overrides) {
            my $src = "$root_dir/libraries/rust/$rel";
            my $src_mtime = (stat($src))[9] // 0;
            if ($src_mtime > $stamp_mtime) {
                $overlay_fresh = 0;
                last;
            }
        }
    }
    if ($overlay_fresh) {
        print "[cache] reuse Rust sysroot overlay\n";
        return;
    }

    my $base_sysroot = capture_stdout('rustc', "+$toolchain", '--print', 'sysroot');
    chomp $base_sysroot;
    dief("rust sysroot not found: $base_sysroot") if !-d $base_sysroot;

    remove_tree("$sysroot_overlay.tmp");
    make_path("$sysroot_overlay.tmp");

    opendir my $root_dh, $base_sysroot or dief("opendir $base_sysroot: $!");
    for my $name (grep {$_ ne '.' && $_ ne '..'} readdir $root_dh) {
        next if $name eq 'bin' || $name eq 'lib';
        symlink("$base_sysroot/$name", "$sysroot_overlay.tmp/$name") or dief("symlink $name: $!");
    }
    closedir $root_dh;

    make_path("$sysroot_overlay.tmp/bin");
    opendir my $bin_dh, "$base_sysroot/bin" or dief("opendir $base_sysroot/bin: $!");
    for my $name (grep {$_ ne '.' && $_ ne '..'} readdir $bin_dh) {
        next if $name eq 'rustc' || $name eq 'rustdoc';
        symlink("$base_sysroot/bin/$name", "$sysroot_overlay.tmp/bin/$name") or dief("symlink bin/$name: $!");
    }
    closedir $bin_dh;

    for my $tool (qw(rustc rustdoc)) {
        open my $fh, '>', "$sysroot_overlay.tmp/bin/$tool" or dief("open overlay $tool: $!");
        print {$fh} "#!/usr/bin/env bash\nset -euo pipefail\nexec \"$base_sysroot/bin/$tool\" --sysroot \"$sysroot_overlay\" \"\$@\"\n";
        close $fh;
        chmod 0755, "$sysroot_overlay.tmp/bin/$tool" or dief("chmod overlay $tool: $!");
    }

    my $rustlib_overlay = "$sysroot_overlay.tmp/lib/rustlib";
    make_path($rustlib_overlay);
    opendir my $rustlib_dh, "$base_sysroot/lib/rustlib" or dief("opendir rustlib: $!");
    for my $name (grep {$_ ne '.' && $_ ne '..'} readdir $rustlib_dh) {
        next if $name eq 'src';
        symlink("$base_sysroot/lib/rustlib/$name", "$rustlib_overlay/$name") or dief("symlink rustlib/$name: $!");
    }
    closedir $rustlib_dh;

    make_path("$rustlib_overlay/src");
    run('cp', '-rs', "$base_sysroot/lib/rustlib/src/rust", "$rustlib_overlay/src/");
    for my $rel (@overrides) {
        my $dst = "$rustlib_overlay/src/rust/$rel";
        unlink $dst or dief("unlink $dst: $!") if -e $dst || -l $dst;
        copy("$root_dir/libraries/rust/$rel", $dst) or dief("copy $rel: $!");
    }
    my $cargo_config_src = "$base_sysroot/lib/rustlib/src/rust/library/.cargo/config.toml";
    my $cargo_config_dst = "$rustlib_overlay/src/rust/library/.cargo/config.toml";
    if (-f $cargo_config_src) {
        unlink $cargo_config_dst or dief("unlink $cargo_config_dst: $!") if -e $cargo_config_dst || -l $cargo_config_dst;
        copy($cargo_config_src, $cargo_config_dst) or dief("copy library .cargo/config.toml: $!");
    }
	my $vendor_root = "$root_dir/libraries/rust/vendor";
	my $literal_escaper_src = "$vendor_root/rustc-literal-escaper";

	if (!-d $literal_escaper_src) {
		print "[fetch] rustc-literal-escaper\n";

		my $fetch_root = "$out_root/rustc-literal-escaper-fetch";
		my $fetch_src = "$fetch_root/src";

		remove_tree($fetch_root);
		make_path($fetch_src);

		open my $manifest, '>', "$fetch_root/Cargo.toml"
			or dief("open $fetch_root/Cargo.toml: $!");
		print {$manifest} <<'EOF';
[package]
name = "mochios-rustc-literal-fetch"
version = "0.0.0"
edition = "2021"

[dependencies]
rustc-literal-escaper = "*"
EOF
		close $manifest;

		open my $lib, '>', "$fetch_src/lib.rs"
			or dief("open $fetch_src/lib.rs: $!");
		close $lib;

		my $cargo_home = $ENV{CARGO_HOME} // "$ENV{HOME}/.cargo";

		run(
			'cargo',
			"+$toolchain",
			'fetch',
			'--manifest-path',
			"$fetch_root/Cargo.toml",
		);

		my @matches = glob(
			"$cargo_home/registry/src/*/rustc-literal-escaper-*"
		);
		dief('downloaded rustc-literal-escaper source was not found')
			if !@matches;

		@matches = sort @matches;
		make_path($vendor_root);
		copy_tree($matches[-1], $literal_escaper_src);
	}

	make_path("$rustlib_overlay/src/rust/vendor");
	copy_tree(
		$literal_escaper_src,
		"$rustlib_overlay/src/rust/vendor/rustc-literal-escaper",
	);
	make_path("$rustlib_overlay/src/rust/library/vendor");

    remove_tree($sysroot_overlay);
    rename "$sysroot_overlay.tmp", $sysroot_overlay or dief("rename sysroot overlay: $!");
    open my $stamp_fh, '>', $overlay_stamp or dief("open overlay stamp: $!");
    print {$stamp_fh} "ok\n";
    close $stamp_fh;
}

sub build_std_binary {
    my ($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, $rustflags, $manifest_path, $binary_name, @features) = @_;
    my $binary_out = "$target_dir/x86_64-unknown-mochios/release/$binary_name";
    my $stable_binary_out = "$stable_target_dir/x86_64-unknown-mochios/release/$binary_name";
    my $rust_src_root = "$sysroot_overlay/lib/rustlib/src/rust";
    my $library_root = "$rust_src_root/library";
    my $vendor_dir = "$library_root/vendor";
    need_file($manifest_path);
    need_dir($vendor_dir);
    need_dir("$library_root/rustc-std-workspace-core");
    need_dir("$library_root/rustc-std-workspace-alloc");
    need_dir("$library_root/rustc-std-workspace-std");
    my @cargo_configs = (
        "patch.crates-io.libc.path='$libc_override_path'",
        "patch.crates-io.rustc-std-workspace-core.path='$library_root/rustc-std-workspace-core'",
        "patch.crates-io.rustc-std-workspace-alloc.path='$library_root/rustc-std-workspace-alloc'",
        "patch.crates-io.rustc-std-workspace-std.path='$library_root/rustc-std-workspace-std'",
    );
    if ($manifest_path eq "$root_dir/binaries/coreutils/Cargo.toml") {
        push @cargo_configs,
            "patch.\"https://github.com/mochiOS/syscalls\".mochios-user-database.path='$root_dir/user/crates/user-database'";
    }
    if (index($manifest_path, "$root_dir/services/") == 0
        || index($manifest_path, '/services-stage/') >= 0) {
        push @cargo_configs,
            "patch.\"https://github.com/mochiOS/syscalls\".mochi-user-platform.path='$root_dir/user/crates/platform'",
            "patch.\"https://github.com/mochiOS/syscalls\".mochios-tls-client.path='$root_dir/user/crates/tls-client'";
    }
    opendir my $vendor_dh, $vendor_dir or dief("opendir $vendor_dir: $!");
    for my $entry (sort grep {$_ ne '.' && $_ ne '..'} readdir $vendor_dh) {
        my $crate_dir = "$vendor_dir/$entry";
        my $manifest = "$crate_dir/Cargo.toml";
        next if !-f $manifest;
        open my $fh, '<', $manifest or dief("open $manifest: $!");
        my $package_name;
        while (my $line = <$fh>) {
            if ($line =~ /^\s*name\s*=\s*"([^"]+)"/) {
                $package_name = $1;
                last;
            }
        }
        close $fh;
        next if !defined $package_name;
        next if $package_name eq 'libc';
        push @cargo_configs, "patch.crates-io.$package_name.path='$crate_dir'";
    }
    closedir $vendor_dh;
    print "[build] $binary_name (std)\n";
    my %cargo_env = (
        RUSTUP_HOME => $rustup_home,
        RUSTFLAGS   => join(' ', @{$rustflags}),
    );
    my @cargo_config_args = map { ('--config', $_) } @cargo_configs;
    my @command = (
        'rustup', 'run', $overlay_toolchain, 'cargo', 'build',
        '-Z', 'build-std=std,panic_abort,compiler_builtins',
        '-Z', 'json-target-spec',
        @cargo_config_args,
        '--manifest-path', $manifest_path,
        '--bin', $binary_name,
        '--release',
        '--target', $target_json,
        '--target-dir', $target_dir,
    );
    push @command, '--features', join(',', @features) if @features;
    run_env(\%cargo_env, @command);
    need_file($binary_out);
    make_path(dirname($stable_binary_out));
    copy($binary_out, $stable_binary_out) or dief("copy $binary_out: $!");
    print "[done] $binary_out\n";
}

sub build_rust_std_programs {
    my ($root_dir, $config, $toolchain, $coreutils_bins) = @_;
    my $user_root = "$root_dir/user";
    my $out_root = "$root_dir/out/rust-std";
    my $target_json = "$user_root/targets/x86_64-unknown-mochios.json";
    my $bootstrap_target = 'x86_64-elf';
    my $sysroot_dir = "$root_dir/out/newlib-port/toolchain/$bootstrap_target";
    my $crt0_o = "$root_dir/out/newlib-port/hello/crt0.o";
    my $runtime_lib = "$root_dir/out/newlib-port/cargo-target/x86_64-unknown-mochios/release/libmochi_user_newlib_runtime.a";
    my $linker_script = "$user_root/runtime/linker.ld";
    my $sysroot_overlay = "$out_root/sysroot-overlay";
    my $libc_override_path = "$root_dir/libraries/libc";
    my $libc_build_hash = capture_stdout('cksum', "$libc_override_path/build.rs");
    $libc_build_hash =~ s/\s.*\z//s;
    my $target_dir = "$out_root/target-libc-patch-$libc_build_hash";
    my $stable_target_dir = "$out_root/target";
    my $services_stage = "$out_root/services-stage";
    my $rustup_home = "$out_root/rustup-home-$libc_build_hash";
    my $overlay_toolchain = "mochios-overlay-$libc_build_hash";

    relink_libc_src($libc_override_path);
    remove_tree($services_stage);
    make_path($services_stage);
    install_file('0644', "$root_dir/services/Cargo.toml", "$services_stage/Cargo.toml");
    install_file('0644', "$root_dir/services/Cargo.lock", "$services_stage/Cargo.lock");
    for my $service (qw(capability compositor core display drivers input logger network package service-manager signature tty update)) {
        copy_tree("$root_dir/services/$service", "$services_stage/$service");
    }

    for my $cmd (qw(cargo rustc rustup x86_64-elf-gcc cksum)) {
        need_cmd($cmd);
    }
    need_file($target_json);
    need_dir("$root_dir/libraries/rust/library");
    need_file("$libc_override_path/Cargo.toml");
    need_file($linker_script);
    need_file($crt0_o);
    need_file($runtime_lib);
    need_dir("$sysroot_dir/lib");

    make_path($out_root);
    prepare_rust_sysroot_overlay($root_dir, $toolchain, $out_root, $sysroot_overlay);
    make_path($rustup_home);
    if (!-e "$rustup_home/toolchains/$overlay_toolchain") {
        run_env({ RUSTUP_HOME => $rustup_home }, 'rustup', 'toolchain', 'link', $overlay_toolchain, $sysroot_overlay);
    }

    my @rustflags = (
        '-C', 'linker=x86_64-elf-gcc',
        '-C', "link-arg=--sysroot=$sysroot_dir",
        '-C', "link-arg=-L$sysroot_dir/lib",
        '-C', 'link-arg=-static',
        '-C', 'link-arg=-nostdlib',
        '-C', 'link-arg=-nostartfiles',
        '-C', "link-arg=-Wl,-T,$linker_script",
        '-C', 'link-arg=-Wl,-no-pie',
        '-C', 'link-arg=-Wl,-z,noexecstack',
        '-C', 'link-arg=-Wl,--start-group',
        '-C', "link-arg=$crt0_o",
        '-C', "link-arg=$runtime_lib",
        '-C', 'link-arg=-lc',
        '-C', 'link-arg=-lm',
        '-C', 'link-arg=-lgcc',
        '-C', 'link-arg=-Wl,--end-group',
    );

    my @std_services = (
        ["$services_stage/core/Cargo.toml", 'core'],
        ["$services_stage/logger/Cargo.toml", 'logger'],
        ["$services_stage/capability/Cargo.toml", 'capability'],
        ["$services_stage/service-manager/Cargo.toml", 'service-manager'],
        ["$services_stage/drivers/Cargo.toml", 'drivers'],
        ["$services_stage/input/Cargo.toml", 'input'],
        ["$services_stage/display/Cargo.toml", 'display'],
        ["$services_stage/network/Cargo.toml", 'network'],
        ["$services_stage/compositor/Cargo.toml", 'compositor'],
        ["$services_stage/package/Cargo.toml", 'package'],
        ["$services_stage/tty/Cargo.toml", 'tty'],
        ["$services_stage/update/Cargo.toml", 'update'],
        ["$services_stage/signature/Cargo.toml", 'signature'],
    );
    for my $service (@std_services) {
        my @features = ();
        if ($service->[1] eq 'network' && ($ENV{MOCHIOS_NETWORK_TEST_WEB_PKI} // '') eq '1') {
            @features = ('test-web-pki');
        }
        build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, @{$service}, @features);
    }

    build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$user_root/apps/rust-std-demo/Cargo.toml", 'rust-std-demo');
    build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$root_dir/applications/test.app/Cargo.toml", 'test_app');
    build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$root_dir/applications/binder/Cargo.toml", 'binder');
    build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$root_dir/applications/terminal/Cargo.toml", 'terminal');
    build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$root_dir/applications/file/Cargo.toml", 'files');
    build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$root_dir/binaries/msh/Cargo.toml", 'msh');
    for my $bin (@{$coreutils_bins}) {
        build_std_binary($root_dir, $rustup_home, $overlay_toolchain, $target_json, $target_dir, $stable_target_dir, $sysroot_overlay, $libc_override_path, \@rustflags, "$root_dir/binaries/coreutils/Cargo.toml", $bin);
    }
}

sub build_cext_module {
    my ($root_dir, $toolchain, $target_dir, $bundles_dir, $package_name, $crate_stem, $bundle_name, @deps) = @_;
    my $cexts_root = "$root_dir/cexts";
    print "[build] cext $bundle_name\n";
    run_env(
        cargo_env(),
        'cargo', "+$toolchain", 'rustc',
        '-Z', 'build-std=core,compiler_builtins',
        '--release',
        '--target', 'x86_64-unknown-none',
        '--target-dir', $target_dir,
        '--manifest-path', "$cexts_root/Cargo.toml",
        '-p', $package_name,
        '--',
        '--emit=obj',
        '-C', 'relocation-model=pic',
        '-C', 'panic=abort',
    );
    my $newest_obj = latest_matching_file("$target_dir/x86_64-unknown-none/release/deps", qr/^\Q$crate_stem\E-.*\.o\z/);
    my $bundle_dir = "$bundles_dir/$bundle_name.cext";
    my $elf_out = "$bundle_dir/$bundle_name.elf";
    my $entry_out = "$bundle_dir/entry";
    my $manifest_src = "$cexts_root/$bundle_name.cext/manifest.toml";
    make_path($bundle_dir);
    run('ld', '-shared', '-nostdlib', '-z', 'noexecstack', '-Bsymbolic', '-o', $elf_out, $newest_obj);
    run('readelf', '-h', $elf_out);
    my $relocs = capture_stdout('readelf', '-rW', $elf_out);
    for my $line (split /\n/, $relocs) {
        dief("unsupported relocation remained in $elf_out") if $line =~ /R_X86_64_/ && $line !~ /\bR_X86_64_RELATIVE\b/;
    }
    my @pack_args = (
        '--name', $bundle_name,
        '--version', '1',
        '--elf', $elf_out,
        '--out', $entry_out,
    );
    for my $dep (@deps) {
        push @pack_args, '--dep', $dep;
    }
    run('perl', "$root_dir/scripts/pack-cext.pl", @pack_args);
    install_file('0644', $manifest_src, "$bundle_dir/manifest.toml");
}

sub build_cexts {
    my ($root_dir, $toolchain) = @_;
    my $cexts_root = "$root_dir/cexts";
    my $out_root = "$root_dir/out/cexts";
    my $target_dir = "$out_root/target";
    my $bundles_dir = "$out_root/bundles";
    for my $cmd (qw(awk cargo find install ld perl readelf)) {
        need_cmd($cmd);
    }
    need_file("$cexts_root/Cargo.toml");
    need_file("$root_dir/scripts/pack-cext.pl");
    need_file("$cexts_root/disk.cext/manifest.toml");
    need_file("$cexts_root/ext2.cext/manifest.toml");
    remove_tree($bundles_dir);
    make_path($bundles_dir);
    build_cext_module($root_dir, $toolchain, $target_dir, $bundles_dir, 'mochi-disk-cext', 'mochi_disk_cext', 'disk');
    build_cext_module($root_dir, $toolchain, $target_dir, $bundles_dir, 'mochi-ext2-cext', 'mochi_ext2_cext', 'ext2', 'disk');
    print "[done] $bundles_dir\n";
}

sub build_staged_cargo_bin {
    my ($toolchain, $target_json, $target_dir, $stage, $package, @features) = @_;
    my @mnu_abi_patch = (
        '--config',
        q{patch.crates-io.mnu-abi.git='https://github.com/mochiOS/mnu'},
    );
    run_env(cargo_env(), 'cargo', "+$toolchain", 'generate-lockfile', @mnu_abi_patch, '--manifest-path', "$stage/Cargo.toml");
    my @cmd = (
        'cargo', "+$toolchain", 'build',
        '-Z', 'build-std=core,alloc,compiler_builtins',
        '-Z', 'json-target-spec',
        '--release',
        '--target', $target_json,
        '--target-dir', $target_dir,
        @mnu_abi_patch,
        '--manifest-path', "$stage/Cargo.toml",
        '-p', $package,
    );
    push @cmd, '--features', join(',', @features) if @features;
    run_env(cargo_env(), @cmd);
}

sub build_driver_bundles {
    my ($root_dir, $config, $toolchain) = @_;
    my $drivers_root = "$root_dir/drivers";
    my $out_root = "$root_dir/out/services-build";
    my $target_dir = "$out_root/target";
    my $target_json = "$root_dir/services/core/x86_64-unknown-mochios.json";
    my $user_root = "$root_dir/user";
    my $plugkit_root = "$root_dir/core/crates/PlugKit/plugkit";
    my $mnu_abi_root = "$root_dir/core/crates/abi";
    need_cmd('cargo');
    need_file($target_json);
    need_file("$drivers_root/usb-driver/Cargo.toml") if config_enabled($config->{DRIVER_XHCI});
    need_file("$drivers_root/ps2/i8042-driver/Cargo.toml") if config_enabled($config->{DRIVER_I8042});
    need_file("$drivers_root/virtio-net-driver/Cargo.toml") if config_enabled($config->{DRIVER_VIRTIO_NET});

    remove_tree("$out_root/stage");

    if (config_enabled($config->{DRIVER_XHCI})) {
        my $stage = "$out_root/stage/usb-driver";
        copy_tree("$drivers_root/usb-driver", $stage);
        unlink "$stage/Cargo.lock" if -e "$stage/Cargo.lock";
        rewrite_cargo_paths("$stage/Cargo.toml", '../..', $user_root, $plugkit_root, $mnu_abi_root);
        print "[build] usb driver bundle\n";
        build_staged_cargo_bin($toolchain, $target_json, $target_dir, $stage, 'usb-driver');
    }
    if (config_enabled($config->{DRIVER_I8042})) {
        my $stage = "$out_root/stage/i8042-driver";
        copy_tree("$drivers_root/ps2/i8042-driver", $stage);
        unlink "$stage/Cargo.lock" if -e "$stage/Cargo.lock";
        rewrite_cargo_paths("$stage/Cargo.toml", '../../..', $user_root, $plugkit_root, $mnu_abi_root);
        print "[build] i8042 driver bundle\n";
        build_staged_cargo_bin($toolchain, $target_json, $target_dir, $stage, 'i8042-driver');
    }
    if (config_enabled($config->{DRIVER_VIRTIO_NET})) {
        my $stage = "$out_root/stage/virtio-net-driver";
        copy_tree("$drivers_root/virtio-net-driver", $stage);
        unlink "$stage/Cargo.lock" if -e "$stage/Cargo.lock";
        rewrite_cargo_paths("$stage/Cargo.toml", '../..', $user_root, $plugkit_root, $mnu_abi_root);
        print "[build] virtio-net driver bundle\n";
        build_staged_cargo_bin($toolchain, $target_json, $target_dir, $stage, 'virtio-net-driver');
    }
    print "[done] $target_dir/x86_64-unknown-mochios/release\n";
}

sub build_bootloader {
    my ($root_dir) = @_;
    my $boot_root = "$root_dir/boot";
    my $target_dir = "$root_dir/out/bootloader/target";
    need_cmd('cargo');
    need_file("$boot_root/Cargo.toml");
    print "[build] bootloader\n";
    run_env(
        cargo_env(RUSTFLAGS => '--cfg curve25519_dalek_backend="serial"'),
        'cargo', '+nightly-2026-05-14', 'build',
        '-Z', 'build-std=core,alloc,compiler_builtins',
        '--release',
        '--target', 'x86_64-unknown-uefi',
        '--target-dir', $target_dir,
        '--manifest-path', "$boot_root/Cargo.toml",
    );
    my $boot_release_dir = "$target_dir/x86_64-unknown-uefi/release";
    dief("bootloader binary was not found in $boot_release_dir") if !-f "$boot_release_dir/boot.efi" && !-f "$boot_release_dir/boot";
    print "[done] $boot_release_dir\n";
}

sub build_fonts {
    my ($root_dir) = @_;
    my $fonts_root = "$root_dir/libraries/fonts";
    need_cmd('make');
    print "[build] fonts\n";
    run_in_dir($root_dir, 'make', 'fonts');
    print "[done] $fonts_root/out/fonts\n";
}

sub stage_cext_bundles {
    my ($cexts_dir, $initfs_stage) = @_;
    dief("bundle directory not found: $cexts_dir") if !-d $cexts_dir;
    make_path($initfs_stage);
    opendir my $dh, $cexts_dir or dief("opendir $cexts_dir: $!");
    my @bundles = sort grep {/\.cext\z/ && -d "$cexts_dir/$_"} readdir $dh;
    closedir $dh;
    my @entries;
    for my $bundle (@bundles) {
        my $bundle_dir = "$cexts_dir/$bundle";
        my $manifest = "$bundle_dir/manifest.toml";
        my $entry = "$bundle_dir/entry";
        need_file($manifest);
        need_file($entry);
        my $target_dir = "$initfs_stage/$bundle";
        remove_tree($target_dir);
        make_path($target_dir);
        install_file('0644', $manifest, "$target_dir/manifest.toml");
        install_file('0644', $entry, "$target_dir/entry");
        push @entries, "/$bundle/manifest.toml=$manifest", "/$bundle/entry=$entry";
    }
    return @entries;
}

sub stage_package_manifest {
    my ($rootfs_stage, $manifest_src, $manifest_dst) = @_;
    return if !defined($manifest_src) || $manifest_src eq '';
    make_path(dirname("$rootfs_stage$manifest_dst"));
    install_file('0644', $manifest_src, "$rootfs_stage$manifest_dst");
}

sub stage_binder_app_bundle {
    my ($rootfs_stage, $path) = @_;
    my $bundle_root = "$rootfs_stage/applications/Binder.app";
    remove_tree($bundle_root);
    make_path($bundle_root);
    install_file('0755', $path->{binder_bin}, "$bundle_root/entry.elf");
    install_file('0644', $path->{binder_about}, "$bundle_root/about.toml");
    install_file('0644', $path->{binder_manifest}, "$bundle_root/manifest.toml");
    for my $resource (qw(appicon.svg close.svg maximize.svg minimize.svg mochios.svg)) {
        install_file('0644', "$path->{binder_resources_dir}/$resource", "$bundle_root/$resource");
    }
}

sub stage_viewkit_test_bundle {
    my ($rootfs_stage, $path) = @_;
    stage_application_bundle(
        $rootfs_stage,
        "$rootfs_stage/applications/test.app",
        $path->{viewkit_test_bin},
        $path->{viewkit_test_about},
        $path->{viewkit_test_manifest},
    );
}

sub stage_terminal_app_bundle {
    my ($rootfs_stage, $path) = @_;
    stage_application_bundle(
        $rootfs_stage,
        "$rootfs_stage/applications/Terminal.app",
        $path->{terminal_bin},
        $path->{terminal_about},
        $path->{terminal_manifest},
    );
    install_file('0644', $path->{terminal_icon}, "$rootfs_stage/applications/Terminal.app/appicon.svg");
}

sub stage_files_app_bundle {
    my ($rootfs_stage, $path) = @_;
    my $bundle_root = "$rootfs_stage/applications/Files.app";
    stage_application_bundle(
        $rootfs_stage,
        $bundle_root,
        $path->{files_bin},
        $path->{files_about},
        $path->{files_manifest},
    );
    install_file('0644', $path->{files_icon}, "$bundle_root/appicon.svg");
    make_path("$bundle_root/icons");
    for my $resource (qw(folder.svg file.svg application.svg image.svg archive.svg disk.svg)) {
        install_file('0644', "$path->{files_icons_dir}/$resource", "$bundle_root/icons/$resource");
    }
}

sub stage_application_bundle {
    my ($rootfs_stage, $bundle_root, $entry_bin, $about_src, $manifest_src) = @_;
    remove_tree($bundle_root);
    make_path($bundle_root);
    install_file('0755', $entry_bin, "$bundle_root/entry.elf");
    install_file('0644', $about_src, "$bundle_root/about.toml");
    install_file('0644', $manifest_src, "$bundle_root/manifest.toml");
}

sub stage_binder_sample_apps {
    my ($rootfs_stage, $source_root) = @_;
    return if !-d $source_root;
    opendir my $dh, $source_root or dief("opendir $source_root: $!");
    my @apps = sort grep {/\.app\z/ && -d "$source_root/$_"} readdir $dh;
    closedir $dh;
    make_path("$rootfs_stage/applications");
    for my $app (@apps) {
        copy_tree("$source_root/$app", "$rootfs_stage/applications/$app");
    }
}

sub stage_driver_bundle {
    my ($rootfs_stage, $manifest_src, $entry_bin, $bundle_root, $entry_name) = @_;
    return if !defined($manifest_src) || $manifest_src eq '' || !defined($entry_bin) || $entry_bin eq '';
    make_path("$rootfs_stage$bundle_root");
    $entry_name //= 'entry.elf';
    install_file('0755', $entry_bin, "$rootfs_stage$bundle_root/$entry_name");
}

sub build_rootfs {
    my ($rootfs_stage, $rootfs_img, $rootfs_size_mb, $path, $coreutils_bin_dir, $coreutils_bins, $config, $mpk_demo_mpkg, $mpk_test_mpkg, $drivers_bundle_root, $i8042_bundle_root, $virtio_net_bundle_root, $fonts_src) = @_;
    need_cmd('mke2fs');
    need_file($path->{hello_elf});
    need_file($path->{signature_db});
    remove_tree($rootfs_stage);
    make_path("$rootfs_stage/bin");
    make_path("$rootfs_stage/tmp");
    make_path("$rootfs_stage/libraries/system");
    make_path("$rootfs_stage/system/logs");
    install_file('0755', $path->{hello_elf}, "$rootfs_stage/bin/hello");
    install_file('0755', $path->{rust_std_demo_bin}, "$rootfs_stage/bin/rust-std-demo");
    install_file('0755', $path->{test_app_bin}, "$rootfs_stage/bin/test_app");
    install_file('0755', $path->{msh_bin}, "$rootfs_stage/bin/msh");
    stage_font_assets($fonts_src, "$rootfs_stage/libraries/fonts");
    stage_resource_tree($path->{resources_dir}, $rootfs_stage);
    chmod 0600, "$rootfs_stage/system/users/users.db"
        or dief("chmod user database: $!");
    for my $directory (qw(Desktop Documents Downloads Movies Music Pictures)) {
        make_path("$rootfs_stage/home/root/$directory");
    }
    make_path("$rootfs_stage/system/resources/msh");
    install_file('0644', $path->{msh_font}, "$rootfs_stage/system/resources/msh/ter-u12b.bdf");
    for my $coreutil (@{$coreutils_bins}) {
        install_file('0755', "$coreutils_bin_dir/$coreutil", "$rootfs_stage/bin/$coreutil");
    }
    install_file(
        '0644',
        $path->{signature_db},
        "$rootfs_stage/libraries/system/execution.allowlist",
    );
    if (!defined($ENV{MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX})
        || $ENV{MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX} eq '') {
        make_path("$rootfs_stage/libraries/certificate");
        install_file('0644', $path->{development_trust_snapshot}, "$rootfs_stage/libraries/certificate/trust-a.json");
        install_file('0644', $path->{development_revocation_snapshot}, "$rootfs_stage/libraries/certificate/revocations-a.json");
    }

    make_path("$rootfs_stage/system/packages");
    if (config_enabled($config->{USER_BUILD_MPK_SAMPLES})) {
        make_path("$rootfs_stage/system/samples");
        install_file('0644', $mpk_demo_mpkg, "$rootfs_stage/system/samples/mpk-demo.mpkg");
        install_file('0644', $mpk_test_mpkg, "$rootfs_stage/system/samples/mpk-test.mpkg");
    }
    stage_package_manifest($rootfs_stage, $path->{rust_std_demo_manifest_src}, '/system/packages/rust-std-demo/manifest.toml');
    stage_package_manifest($rootfs_stage, $path->{viewkit_test_manifest}, '/system/packages/viewkit-test/manifest.toml');
    stage_package_manifest($rootfs_stage, $path->{msh_manifest}, '/system/packages/msh/manifest.toml');
    stage_package_manifest($rootfs_stage, $path->{coreutils_manifest}, '/system/packages/coreutils/manifest.toml');
    stage_package_manifest($rootfs_stage, $path->{binder_manifest}, '/system/packages/binder/manifest.toml');
    stage_package_manifest($rootfs_stage, $path->{terminal_manifest}, '/system/packages/terminal/manifest.toml');
    stage_package_manifest($rootfs_stage, $path->{files_manifest}, '/system/packages/files/manifest.toml');
    stage_binder_sample_apps($rootfs_stage, $path->{binder_sample_apps_dir});
    stage_binder_app_bundle($rootfs_stage, $path);
    stage_viewkit_test_bundle($rootfs_stage, $path);
    stage_terminal_app_bundle($rootfs_stage, $path);
    stage_files_app_bundle($rootfs_stage, $path);

    make_path("$rootfs_stage/system/services");
    for my $service (qw(capability display compositor drivers logger input network package signature tty)) {
        my $service_name = $service eq 'display' ? 'display.driver' : "$service.service";
        install_file('0755', $path->{"${service}_service_bin"}, "$rootfs_stage/system/services/$service_name");
        stage_package_manifest($rootfs_stage, $path->{"${service}_service_manifest"}, "/system/packages/$service/manifest.toml");
    }
    install_file('0755', $path->{service_manager_service_bin}, "$rootfs_stage/system/services/service-manager.service");
    stage_package_manifest($rootfs_stage, $path->{service_manager_service_manifest}, '/system/packages/service-manager/manifest.toml');
    install_file('0755', $path->{update_service_bin}, "$rootfs_stage/system/services/update.service");
    stage_package_manifest($rootfs_stage, $path->{update_service_manifest}, '/system/packages/update/manifest.toml');
    if (config_enabled($config->{DRIVER_XHCI})) {
        stage_package_manifest($rootfs_stage, $path->{usb_driver_manifest}, "/system/packages@{[ $drivers_bundle_root =~ s#^/bin##r ]}/manifest.toml");
        stage_driver_bundle($rootfs_stage, $path->{usb_driver_manifest}, $path->{usb_driver_bin}, $drivers_bundle_root);
    }
    if (config_enabled($config->{DRIVER_I8042})) {
        stage_package_manifest($rootfs_stage, $path->{i8042_driver_manifest}, "/system/packages@{[ $i8042_bundle_root =~ s#^/bin##r ]}/manifest.toml");
        stage_driver_bundle($rootfs_stage, $path->{i8042_driver_manifest}, $path->{i8042_driver_bin}, $i8042_bundle_root);
    }
    if (config_enabled($config->{DRIVER_VIRTIO_NET})) {
        stage_package_manifest($rootfs_stage, $path->{virtio_net_driver_manifest}, "/system/packages@{[ $virtio_net_bundle_root =~ s#^/bin##r ]}/manifest.toml");
        stage_driver_bundle($rootfs_stage, $path->{virtio_net_driver_manifest}, $path->{virtio_net_driver_bin}, $virtio_net_bundle_root, 'virtio-net.driver');
    }

    opendir my $root_dh, $rootfs_stage or dief("opendir $rootfs_stage: $!");
    my @root_files = sort grep {
        $_ ne '.' && $_ ne '..' && !-d "$rootfs_stage/$_"
    } readdir $root_dh;
    closedir $root_dh;
    dief("rootfs root must contain directories only: @root_files") if @root_files;

    unlink $rootfs_img if -e $rootfs_img;
    run('truncate', '-s', "${rootfs_size_mb}M", $rootfs_img);
    run('mke2fs', '-q', '-t', 'ext2', '-b', '4096', '-d', $rootfs_stage, '-F', $rootfs_img);
}

sub write_gpt {
    my ($disk_img, $esp_start, $esp_size, $rootfs_start, $rootfs_size) = @_;
    open my $oldout, '>&', \*STDOUT or dief("dup stdout: $!");
    open STDOUT, '>', '/dev/null' or dief("redirect stdout: $!");
    open my $fh, '|-', 'sfdisk', $disk_img or dief("spawn sfdisk: $!");
    print {$fh} "label: gpt\n";
    print {$fh} "unit: sectors\n";
    print {$fh} "first-lba: 2048\n";
    print {$fh} "sector-size: 512\n\n";
    print {$fh} "$esp_start,$esp_size,U,*\n";
    print {$fh} "$rootfs_start,$rootfs_size,L\n";
    my $ok = close $fh;
    open STDOUT, '>&', $oldout or dief("restore stdout: $!");
    close $oldout;
    dief("sfdisk failed") if !$ok;
}

sub relink_libc_src {
	my ($libc_root) = @_;

	my $cargo_home = $ENV{CARGO_HOME} // "$ENV{HOME}/.cargo";
	my @matches = sort glob(
		"$cargo_home/registry/src/*/libc-0.2.185"
	);

	my $registry_root;

	for my $candidate (@matches) {
		my $mod_path =
			"$candidate/src/unix/newlib/mod.rs";

		next if !-f $mod_path;

		open my $fh, '<', $mod_path
			or dief("open $mod_path: $!");

		local $/;
		my $source = <$fh> // '';
		close $fh;

		next if $source !~ /\bmod generic;/;
		next if $source !~ /target_arch = "aarch64"/;
		next if $source !~ /target_arch_not_implemented/;

		$registry_root = $candidate;
		last;
	}

	dief('valid libc 0.2.185 source was not found in Cargo registry')
		if !defined $registry_root;

	my $registry_src = "$registry_root/src";
	my $override_src = "$libc_root/newlib";
	my $libc_src = "$libc_root/src";
	my $generated_root = "$libc_root/.generated";
	my $upstream_mod =
		"$registry_src/unix/newlib/mod.rs";

	need_file("$override_src/mod.rs");
	need_file("$override_src/generic.rs");
	need_file("$override_src/mochios.rs");
	need_file($upstream_mod);

	remove_tree($generated_root);
	make_path($generated_root);

	run('cp', '-p', $upstream_mod, "$generated_root/upstream-newlib-mod.rs");

	remove_tree($libc_src);
	run('cp', '-rs', $registry_src, $libc_root);

	my $wrapper_dst = "$libc_src/unix/newlib/mod.rs";

	unlink $wrapper_dst
		or dief("unlink $wrapper_dst: $!")
		if -e $wrapper_dst || -l $wrapper_dst;

	run('cp', '-p', "$override_src/mod.rs", $wrapper_dst);

	print "[link] prepared libc sources\n";
}

if (!-f $config_file) {
    run(
        'perl',
        "$script_dir/config/merge-config.pl",
        '--default',
        "$root_dir/build/defaults.config",
        '--in',
        $config_file,
        '--out',
        $config_file,
        '--mk',
        "$root_dir/build/config.mk",
    );
}

my %config = read_config($config_file);

my $kernel_target = 'x86_64-unknown-none';
my $nightly_toolchain = $config{KERNEL_RUST_TOOLCHAIN};
my $build_root = "$root_dir/out/image-build";
my $artifact_dir = "$root_dir/out/artifacts";
my $build_input_stamp = "$root_dir/out/.build-input-stamp";
my $esp_dir = "$build_root/esp";
my $esp_img = "$build_root/esp.img";
my $disk_img = "$build_root/disk.img";
my $initfs_stage = "$build_root/initfs-root";
my $initfs_img = "$build_root/initfs.img";
my $rootfs_stage = "$build_root/rootfs-root";
my $rootfs_img = "$build_root/rootfs.img";
my $signature_db_stage = "$build_root/execution.allowlist";
my $cext_bundles_dir = "$root_dir/out/cexts/bundles";
my $drivers_bundle_root = '/bin/drivers/usb/qemu-usb.driver';
my $i8042_bundle_root = '/bin/drivers/ps2/i8042.driver';
my $virtio_net_bundle_root = '/bin/drivers/network/virtio-net.driver';
my $enable_xhci = config_to_01($config{DRIVER_XHCI});
my $enable_i8042 = config_to_01($config{DRIVER_I8042});
my $enable_virtio_net = config_to_01($config{DRIVER_VIRTIO_NET});
my $coreutils_bin_dir = "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release";
my $mpk_demo_mpkg = "$build_root/mpk-demo.mpkg";
my $mpk_test_mpkg = "$build_root/mpk-test.mpkg";
my $devkit_root = "$root_dir/tools/devkit";
my $development_fixture_root = "$devkit_root/fixtures/development";
my $development_certificate = "$build_root/developer.cert";
my $msign_bin = "$root_dir/out/devkit-target/release/msign";
my @coreutils_bins = qw(echo ls pwd true false cat touch rm id useradd userdel userlist mpk net test_gui test_desktop);
push @coreutils_bins, qw(selftest-capability selftest-process selftest-ext2-write)
    if config_enabled($config{USER_BUILD_SELFTESTS});

if (cached_artifacts_current($root_dir, $build_input_stamp, "$artifact_dir/disk.img")) {
    print "[cache] reuse complete image: $artifact_dir/disk.img\n";
    exit 0;
}

for my $cmd (qw(cargo cp install mcopy mke2fs mkfs.fat mmd perl tar repo sha256sum sfdisk truncate dd find sort)) {
    need_cmd($cmd);
}

need_dir("$root_dir/.repo");
need_dir("$root_dir/libraries/fonts");
need_file("$core_root/Cargo.toml");
for my $script (qw(build-signature-db.pl build-sample-mpkg.pl pack-cext.pl)) {
    need_file("$script_dir/$script");
}

if ($config{IMAGE_DISK_SIZE_MB} <= $config{IMAGE_ESP_SIZE_MB} + 2) {
    dief('IMAGE_DISK_SIZE_MB must be larger than IMAGE_ESP_SIZE_MB + 2');
}
my $rootfs_part_size_mb = $config{IMAGE_DISK_SIZE_MB} - $config{IMAGE_ESP_SIZE_MB} - 2;

print "[clean] build directories\n";
remove_tree($build_root, $artifact_dir);
make_path("$esp_dir/EFI/BOOT", "$esp_dir/system", $initfs_stage, $rootfs_stage, $artifact_dir);

print "[step] build user runtime and newlib\n";
build_newlib_runtime($root_dir, $nightly_toolchain);

print "[step] build Rust std user programs\n";
build_rust_std_programs($root_dir, \%config, $config{USER_RUST_STD_TOOLCHAIN}, \@coreutils_bins);

if (config_enabled($config{USER_BUILD_MPK_SAMPLES})) {
    print "[step] build sample mpkg\n";
    need_file("$devkit_root/Cargo.toml");
    need_file("$development_fixture_root/developer.key");
    need_file("$development_fixture_root/developer.cert.b64");
    run(
        'cargo', 'build', '--release',
        '--manifest-path', "$devkit_root/Cargo.toml",
        '--target-dir', "$root_dir/out/devkit-target",
        '-p', 'msign',
    );
    need_file($msign_bin);
    open my $certificate_in, '<', "$development_fixture_root/developer.cert.b64"
        or dief("open development certificate: $!");
    local $/;
    my $certificate_base64 = <$certificate_in> // '';
    close $certificate_in;
    my $certificate_bytes = decode_base64($certificate_base64);
    length($certificate_bytes) > 0 or dief('development certificate is empty');
    open my $certificate_out, '>', $development_certificate
        or dief("write development certificate: $!");
    binmode $certificate_out;
    print {$certificate_out} $certificate_bytes
        or dief("write development certificate: $!");
    close $certificate_out or dief("close development certificate: $!");
    run(
        'perl',
        "$script_dir/build-sample-mpkg.pl",
        '--output',
        $mpk_demo_mpkg,
        '--payload-bin',
        "$coreutils_bin_dir/echo",
        '--binary-path',
        '/bin/mpk-demo',
        '--package-id',
        'org.mochios.mpkdemo',
        '--package-name',
        'mpk-demo',
        '--package-version',
        '0.1.0',
        '--vendor',
        'mochiOS Project',
    );
    need_file($mpk_demo_mpkg);
    run(
        $msign_bin, 'package', 'sign', $mpk_demo_mpkg,
        '--certificate', $development_certificate,
        '--key', "$development_fixture_root/developer.key",
    );
    run(
        'perl',
        "$script_dir/build-sample-mpkg.pl",
        '--output',
        $mpk_test_mpkg,
        '--payload-bin',
        "$coreutils_bin_dir/echo",
        '--binary-path',
        '/bin/mpk-test',
        '--package-id',
        'org.mochios.tests.mpk',
        '--package-name',
        'mpk-test',
        '--package-version',
        '0.1.0',
        '--vendor',
        'mochiOS Project',
    );
    need_file($mpk_test_mpkg);
    run(
        $msign_bin, 'package', 'sign', $mpk_test_mpkg,
        '--certificate', $development_certificate,
        '--key', "$development_fixture_root/developer.key",
    );
}

print "[step] build cext bundles\n";
build_cexts($root_dir, $nightly_toolchain);

print "[step] build kernel\n";
run_env(
    cargo_env(RUSTFLAGS => '--cfg curve25519_dalek_backend="serial"'),
    'cargo',
    "+$nightly_toolchain",
    'build',
    '-Z',
    'build-std=core,alloc,compiler_builtins',
    '--release',
    '--target',
    $kernel_target,
    '--features',
    'kernel-bin',
    '--manifest-path',
    "$core_root/Cargo.toml",
);

print "[step] build driver bundles\n";
build_driver_bundles($root_dir, \%config, $nightly_toolchain);

print "[step] build bootloader\n";
build_bootloader($root_dir);

print "[step] build fonts\n";
build_fonts($root_dir);

my %path = (
    hello_elf                   => "$root_dir/out/newlib-port/hello/hello.elf",
    rust_std_demo_bin           => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/rust-std-demo",
    rust_std_demo_manifest_src  => "$root_dir/user/apps/rust-std-demo/manifest.toml",
    viewkit_test_bin            => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/test_app",
    viewkit_test_about          => "$root_dir/applications/test.app/about.toml",
    viewkit_test_manifest       => "$root_dir/applications/test.app/manifest.toml",
    kernel_bin                  => "$core_root/target/$kernel_target/release/kernel",
    service_bin                 => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/core",
    drivers_service_bin         => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/drivers",
    compositor_service_bin      => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/compositor",
    display_service_bin         => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/display",
    capability_service_bin      => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/capability",
    logger_service_bin          => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/logger",
    input_service_bin           => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/input",
    network_service_bin         => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/network",
    tty_service_bin             => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/tty",
    package_service_bin         => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/package",
    signature_service_bin       => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/signature",
    service_manager_service_bin => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/service-manager",
    update_service_bin          => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/update",
    drivers_service_manifest    => "$root_dir/services/drivers/manifest.toml",
    compositor_service_manifest => "$root_dir/services/compositor/manifest.toml",
    display_service_manifest    => "$root_dir/services/display/manifest.toml",
    capability_service_manifest => "$root_dir/services/capability/manifest.toml",
    logger_service_manifest     => "$root_dir/services/logger/manifest.toml",
    input_service_manifest      => "$root_dir/services/input/manifest.toml",
    network_service_manifest    => "$root_dir/services/network/manifest.toml",
    package_service_manifest    => "$root_dir/services/package/manifest.toml",
    signature_service_manifest  => "$root_dir/services/signature/manifest.toml",
    service_manager_service_manifest => "$root_dir/services/service-manager/manifest.toml",
    update_service_manifest     => "$root_dir/services/update/manifest.toml",
    tty_service_manifest        => "$root_dir/services/tty/manifest.toml",
    usb_driver_bin              => "$root_dir/out/services-build/target/x86_64-unknown-mochios/release/entry",
    usb_driver_manifest         => "$root_dir/drivers/usb-driver/manifest.toml",
    i8042_driver_bin            => "$root_dir/out/services-build/target/x86_64-unknown-mochios/release/i8042-entry",
    i8042_driver_manifest       => "$root_dir/drivers/ps2/i8042-driver/manifest.toml",
    virtio_net_driver_bin       => "$root_dir/out/services-build/target/x86_64-unknown-mochios/release/virtio-net-driver",
    virtio_net_driver_manifest  => "$root_dir/drivers/virtio-net-driver/manifest.toml",
    binder_bin             => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/binder",
    binder_about           => "$root_dir/applications/binder/about.toml",
    binder_manifest        => "$root_dir/applications/binder/manifest.toml",
    binder_resources_dir   => "$root_dir/applications/binder/resources",
    binder_sample_apps_dir => "$root_dir/applications/binder/resources/apps",
    terminal_bin           => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/terminal",
    terminal_about         => "$root_dir/applications/terminal/about.toml",
    terminal_manifest      => "$root_dir/applications/terminal/manifest.toml",
    terminal_icon          => "$root_dir/applications/terminal/appicon.svg",
    files_bin              => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/files",
    files_about            => "$root_dir/applications/file/about.toml",
    files_manifest         => "$root_dir/applications/file/manifest.toml",
    files_icon             => "$root_dir/applications/file/appicon.svg",
    files_icons_dir        => "$root_dir/applications/file/resources/icons",
    resources_dir          => "$root_dir/resources",
    test_app_bin           => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/test_app",
    msh_bin                     => "$root_dir/out/rust-std/target/x86_64-unknown-mochios/release/msh",
    msh_manifest                => "$root_dir/binaries/msh/manifest.toml",
    msh_font                    => "$root_dir/binaries/msh/resources/ter-u12b.bdf",
    coreutils_manifest          => "$root_dir/binaries/coreutils/manifest.toml",
    signature_db                => $signature_db_stage,
    development_trust_snapshot => "$development_fixture_root/trust-a.json",
    development_revocation_snapshot => "$development_fixture_root/revocations-a.json",
);

my $boot_release_dir = "$root_dir/out/bootloader/target/x86_64-unknown-uefi/release";
my $boot_bin;
if (-f "$boot_release_dir/boot.efi") {
    $boot_bin = "$boot_release_dir/boot.efi";
}
elsif (-f "$boot_release_dir/boot") {
    $boot_bin = "$boot_release_dir/boot";
}
else {
    dief("bootloader binary was not found in $boot_release_dir");
}

for my $key (
    qw(hello_elf rust_std_demo_bin rust_std_demo_manifest_src viewkit_test_bin viewkit_test_about viewkit_test_manifest test_app_bin binder_bin binder_about binder_manifest terminal_bin terminal_about terminal_manifest terminal_icon files_bin files_about files_manifest files_icon kernel_bin service_bin drivers_service_bin compositor_service_bin display_service_bin capability_service_bin logger_service_bin input_service_bin network_service_bin package_service_bin signature_service_bin service_manager_service_bin update_service_bin tty_service_bin drivers_service_manifest compositor_service_manifest display_service_manifest capability_service_manifest logger_service_manifest input_service_manifest network_service_manifest package_service_manifest signature_service_manifest service_manager_service_manifest update_service_manifest tty_service_manifest msh_bin msh_manifest msh_font coreutils_manifest development_trust_snapshot development_revocation_snapshot)
) {
    need_file($path{$key});
}
need_dir($path{binder_resources_dir});
need_dir($path{files_icons_dir});
need_dir($path{resources_dir});
for my $coreutil (@coreutils_bins) {
    need_file("$coreutils_bin_dir/$coreutil");
}
if ($enable_xhci eq '1') {
    need_file($path{usb_driver_bin});
    need_file($path{usb_driver_manifest});
}
if ($enable_i8042 eq '1') {
    need_file($path{i8042_driver_bin});
    need_file($path{i8042_driver_manifest});
}
if ($enable_virtio_net eq '1') {
    need_file($path{virtio_net_driver_bin});
    need_file($path{virtio_net_driver_manifest});
}
need_file($boot_bin);
need_dir($cext_bundles_dir);

print "[step] stage initfs\n";
remove_tree($initfs_stage);
make_path($initfs_stage);
install_file('0755', $path{service_bin}, "$initfs_stage/core.service");
make_path("$initfs_stage/config");
install_file('0644', "$core_root/config/kernel.conf", "$initfs_stage/config/kernel.conf");
my @cext_signature_entries = stage_cext_bundles($cext_bundles_dir, $initfs_stage);
for my $unexpected (qw(bin captest.bin unsigned.bin plugkit testdata hello.txt)) {
    dief("unexpected initfs payload: $unexpected") if -e "$initfs_stage/$unexpected";
}

print "[step] build signature database\n";
my @signature_db_args = (
    '--output',
    $signature_db_stage,
    '--entry',
    "core.service=$path{service_bin}",
    '--entry',
    "/system/services/capability.service=$path{capability_service_bin}",
    '--entry',
    "/system/services/drivers.service=$path{drivers_service_bin}",
    '--entry',
    "/system/services/display.driver=$path{display_service_bin}",
    '--entry',
    "/system/services/compositor.service=$path{compositor_service_bin}",
    '--entry',
    "/system/services/logger.service=$path{logger_service_bin}",
    '--entry',
    "/system/services/input.service=$path{input_service_bin}",
    '--entry',
    "/system/services/network.service=$path{network_service_bin}",
    '--entry',
    "/system/services/package.service=$path{package_service_bin}",
    '--entry',
    "/system/services/signature.service=$path{signature_service_bin}",
    '--entry',
    "/system/services/service-manager.service=$path{service_manager_service_bin}",
    '--entry',
    "/system/services/update.service=$path{update_service_bin}",
    '--entry',
    "/system/services/tty.service=$path{tty_service_bin}",
    '--entry',
    "/bin/hello=$path{hello_elf}",
    '--entry',
    "/bin/rust-std-demo=$path{rust_std_demo_bin}",
    '--entry',
    "/applications/Binder.app/entry.elf=$path{binder_bin}",
    '--entry',
    "/applications/test.app/entry.elf=$path{viewkit_test_bin}",
    '--entry',
    "/applications/Terminal.app/entry.elf=$path{terminal_bin}",
    '--entry',
    "/applications/Files.app/entry.elf=$path{files_bin}",
    '--entry',
    "/bin/msh=$path{msh_bin}",
);
for my $bin (qw(echo ls pwd true false cat touch rm id useradd userdel userlist mpk net test_gui test_app test_desktop)) {
    my $bin_path = $bin eq 'test_app' ? $path{test_app_bin} : "$coreutils_bin_dir/$bin";
    push @signature_db_args, '--entry', "/bin/$bin=$bin_path";
}
if (config_enabled($config{USER_BUILD_SELFTESTS})) {
    push @signature_db_args, '--entry', "/bin/selftest-capability=$coreutils_bin_dir/selftest-capability";
    push @signature_db_args, '--entry', "/bin/selftest-process=$coreutils_bin_dir/selftest-process";
    push @signature_db_args, '--entry', "/bin/selftest-ext2-write=$coreutils_bin_dir/selftest-ext2-write";
}
push @signature_db_args, '--entry', "$drivers_bundle_root/entry.elf=$path{usb_driver_bin}"
    if $enable_xhci eq '1';
push @signature_db_args, '--entry', "$i8042_bundle_root/entry.elf=$path{i8042_driver_bin}"
    if $enable_i8042 eq '1';
push @signature_db_args, '--entry', "$virtio_net_bundle_root/virtio-net.driver=$path{virtio_net_driver_bin}"
    if $enable_virtio_net eq '1';
for my $entry (@cext_signature_entries) {
    push @signature_db_args, '--entry', $entry;
}
run('perl', "$script_dir/build-signature-db.pl", @signature_db_args);
open my $sig_fh, '<', $signature_db_stage or dief("open $signature_db_stage: $!");
my $mpk_record = 0;
while (my $line = <$sig_fh>) {
    if ($line =~ /^record \/bin\/mpk /) {
        $mpk_record = 1;
        last;
    }
}
close $sig_fh;
dief('signature db missing /bin/mpk') if !$mpk_record;

print "[step] build rootfs\n";
build_rootfs(
    $rootfs_stage,
    $rootfs_img,
    $rootfs_part_size_mb,
    \%path,
    $coreutils_bin_dir,
    \@coreutils_bins,
    \%config,
    $mpk_demo_mpkg,
    $mpk_test_mpkg,
    $drivers_bundle_root,
    $i8042_bundle_root,
    $virtio_net_bundle_root,
    "$root_dir/libraries/fonts/out/fonts",
);

print "[step] build initfs image\n";
run('truncate', '-s', "$config{IMAGE_INITFS_SIZE_MB}M", $initfs_img);
run('mke2fs', '-q', '-t', 'ext2', '-b', '1024', '-d', $initfs_stage, '-F', $initfs_img);

print "[step] build esp image\n";
remove_tree($esp_dir);
make_path("$esp_dir/EFI/BOOT", "$esp_dir/system");
install_file('0644', $boot_bin, "$esp_dir/EFI/BOOT/BOOTX64.EFI");
install_file('0644', $path{kernel_bin}, "$esp_dir/system/kernel.elf");
install_file('0644', $initfs_img, "$esp_dir/system/initfs.img");
run('truncate', '-s', "$config{IMAGE_ESP_SIZE_MB}M", $esp_img);
run_quiet('mkfs.fat', '-F', '32', '-n', 'EFI', $esp_img);
my $mtools_env = { MTOOLS_SKIP_CHECK => '1' };
run_env($mtools_env, 'mmd', '-i', $esp_img, '::/EFI');
run_env($mtools_env, 'mmd', '-i', $esp_img, '::/EFI/BOOT');
run_env($mtools_env, 'mmd', '-i', $esp_img, '::/system');
run_env($mtools_env, 'mcopy', '-i', $esp_img, "$esp_dir/EFI/BOOT/BOOTX64.EFI", '::/EFI/BOOT/BOOTX64.EFI');
run_env($mtools_env, 'mcopy', '-i', $esp_img, "$esp_dir/system/kernel.elf", '::/system/kernel.elf');
run_env($mtools_env, 'mcopy', '-i', $esp_img, "$esp_dir/system/initfs.img", '::/system/initfs.img');

print "[step] build GPT disk image\n";
my $esp_start_sector = 2048;
my $esp_size_sectors = $config{IMAGE_ESP_SIZE_MB} * 2048;
my $rootfs_start_sector = $esp_start_sector + $esp_size_sectors;
my $rootfs_size_sectors = $rootfs_part_size_mb * 2048;
unlink $disk_img if -e $disk_img;
run('truncate', '-s', "$config{IMAGE_DISK_SIZE_MB}M", $disk_img);
write_gpt($disk_img, $esp_start_sector, $esp_size_sectors, $rootfs_start_sector, $rootfs_size_sectors);
run('dd', "if=$esp_img", "of=$disk_img", 'bs=512', "seek=$esp_start_sector", 'conv=notrunc', 'status=none');
run('dd', "if=$rootfs_img", "of=$disk_img", 'bs=512', "seek=$rootfs_start_sector", 'conv=notrunc', 'status=none');

print "[step] collect artifacts\n";
install_file('0644', $disk_img, "$artifact_dir/disk.img");
install_file('0644', $initfs_img, "$artifact_dir/initfs.img");
install_file('0644', $path{kernel_bin}, "$artifact_dir/kernel.elf");
install_file('0644', $boot_bin, "$artifact_dir/BOOTX64.EFI");
install_file('0755', $path{service_bin}, "$artifact_dir/core.service");
install_file('0755', $path{capability_service_bin}, "$artifact_dir/capability.service");
install_file('0755', $path{service_manager_service_bin}, "$artifact_dir/service-manager.service");
install_file('0755', $path{update_service_bin}, "$artifact_dir/update.service");
install_file('0755', $path{input_service_bin}, "$artifact_dir/input.service");
install_file('0755', $path{network_service_bin}, "$artifact_dir/network.service");
install_file('0755', $path{tty_service_bin}, "$artifact_dir/tty.service");
install_file('0755', $path{logger_service_bin}, "$artifact_dir/logger.service");
install_file('0755', $path{rust_std_demo_bin}, "$artifact_dir/rust-std-demo");
install_file('0755', $path{test_app_bin}, "$artifact_dir/test_app");
install_file('0755', $path{binder_bin}, "$artifact_dir/binder");
install_file('0755', $path{terminal_bin}, "$artifact_dir/terminal");
install_file('0755', $path{files_bin}, "$artifact_dir/files");
install_file('0755', $path{msh_bin}, "$artifact_dir/msh");
for my $coreutil (@coreutils_bins) {
    install_file('0755', "$coreutils_bin_dir/$coreutil", "$artifact_dir/$coreutil");
}
if (config_enabled($config{USER_BUILD_MPK_SAMPLES})) {
    install_file('0644', $mpk_demo_mpkg, "$artifact_dir/mpk-demo.mpkg");
    install_file('0644', $mpk_test_mpkg, "$artifact_dir/mpk-test.mpkg");
}
install_file('0644', $signature_db_stage, "$artifact_dir/execution.allowlist");
install_file('0755', $path{drivers_service_bin}, "$artifact_dir/drivers.service");
install_file('0755', $path{usb_driver_bin}, "$artifact_dir/usb-driver.entry") if $enable_xhci eq '1';
install_file('0755', $path{i8042_driver_bin}, "$artifact_dir/i8042-driver.entry") if $enable_i8042 eq '1';
install_file('0755', $path{virtio_net_driver_bin}, "$artifact_dir/virtio-net.driver") if $enable_virtio_net eq '1';

print "[step] record exact repo manifest\n";
run_in_dir($root_dir, 'repo', 'manifest', '-r', '-o', "$artifact_dir/manifest.xml");

write_build_info("$artifact_dir/build-info.txt", $root_dir);

print "[step] generate checksums\n";
my @checksum_files = qw(
    disk.img
    initfs.img
    kernel.elf
    BOOTX64.EFI
    core.service
    service-manager.service
    update.service
    drivers.service
    input.service
    network.service
    tty.service
    msh
    terminal
    files
    ls
    rust-std-demo
    mpk
    execution.allowlist
    manifest.xml
    build-info.txt
);
push @checksum_files, 'i8042-driver.entry' if $enable_i8042 eq '1';
push @checksum_files, 'virtio-net.driver' if $enable_virtio_net eq '1';
push @checksum_files, qw(mpk-demo.mpkg mpk-test.mpkg) if config_enabled($config{USER_BUILD_MPK_SAMPLES});
write_checksums($artifact_dir, @checksum_files);
append_checksum($artifact_dir, 'usb-driver.entry') if $enable_xhci eq '1';

print "[done] artifacts:\n";
opendir my $dh, $artifact_dir or dief("opendir $artifact_dir: $!");
for my $name (sort grep {-f "$artifact_dir/$_"} readdir $dh) {
    print "  $name\n";
}
closedir $dh;

open my $stamp_fh, '>', $build_input_stamp
    or dief("write build input stamp $build_input_stamp: $!");
print {$stamp_fh} "ok\n";
close $stamp_fh or dief("close build input stamp $build_input_stamp: $!");
