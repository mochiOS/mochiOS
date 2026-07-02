#!/usr/bin/env perl

use strict;
use warnings;

use File::Basename qw(dirname);
use File::Copy qw(copy);
use File::Path qw(make_path);
use File::Temp qw(tempdir);

sub die_usage {
    die "usage: $0 --output PATH --payload-bin FILE --binary-path PATH [--package-id ID] [--package-name NAME] [--package-version VER] [--vendor VENDOR]\n";
}

sub run_cmd {
    my (@cmd) = @_;
    system(@cmd) == 0 or die "command failed: @cmd\n";
}

sub run_cmd_capture {
    my (@cmd) = @_;
    open(my $fh, '-|', @cmd) or die "failed to run @cmd: $!\n";
    local $/;
    my $data = <$fh>;
    close($fh) or die "command failed: @cmd\n";
    return $data;
}

my $output;
my $payload_bin;
my $binary_path;
my $package_id = 'org.mochios.mpkdemo';
my $package_name = 'mpk-demo';
my $package_version = '0.1.0';
my $vendor = 'mochiOS Project';

while (@ARGV) {
    my $arg = shift @ARGV;
    if ($arg eq '--output') {
        @ARGV or die_usage();
        $output = shift @ARGV;
    }
    elsif ($arg eq '--payload-bin') {
        @ARGV or die_usage();
        $payload_bin = shift @ARGV;
    }
    elsif ($arg eq '--binary-path') {
        @ARGV or die_usage();
        $binary_path = shift @ARGV;
    }
    elsif ($arg eq '--package-id') {
        @ARGV or die_usage();
        $package_id = shift @ARGV;
    }
    elsif ($arg eq '--package-name') {
        @ARGV or die_usage();
        $package_name = shift @ARGV;
    }
    elsif ($arg eq '--package-version') {
        @ARGV or die_usage();
        $package_version = shift @ARGV;
    }
    elsif ($arg eq '--vendor') {
        @ARGV or die_usage();
        $vendor = shift @ARGV;
    }
    else {
        die_usage();
    }
}

defined $output && defined $payload_bin && defined $binary_path or die_usage();
-f $payload_bin or die "missing payload binary: $payload_bin\n";
length($binary_path) && $binary_path =~ m{^/} or die "binary path must be absolute\n";

my $tmpdir = tempdir(CLEANUP => 1);
my $pkg_root = "$tmpdir/pkg";
my $payload_root = "$pkg_root/payload/root";
my $signatures_root = "$pkg_root/signatures";
my $private_key = "$tmpdir/ed25519.key";
my $pub_der = "$tmpdir/pub.der";
my $digest = "$tmpdir/digest.bin";
my $message = "$tmpdir/message.bin";
my $sig = "$tmpdir/manifest.sig";
my $tarfile = "$tmpdir/payload.tar";

make_path("$payload_root", "$signatures_root") or die "failed to create package root\n";

my $payload_path = $payload_root . $binary_path;
my $payload_dir = dirname($payload_path);
make_path($payload_dir) or die "failed to create payload dir\n";
copy($payload_bin, $payload_path) or die "failed to copy payload binary: $!\n";
chmod 0755, $payload_path;

open(my $manifest_fh, '>', "$pkg_root/manifest.toml") or die "failed to open manifest\n";
binmode($manifest_fh) or die "failed to set binary mode on manifest\n";
print {$manifest_fh} <<EOF;
format = 1

[package]
id = "$package_id"
name = "$package_name"
version = "$package_version"
revision = 1
vendor = "$vendor"
kind = "binary"
architecture = "x86_64"
abi = "mochios-1"

[[binary]]
path = "$binary_path"
kind = "application"
EOF
close($manifest_fh) or die "failed to close manifest\n";

run_cmd('openssl', 'genpkey', '-algorithm', 'ed25519', '-out', $private_key);
my $pubout = run_cmd_capture('openssl', 'pkey', '-in', $private_key, '-pubout', '-outform', 'DER');
length($pubout) >= 32 or die "public key output too short\n";
open(my $pub_fh, '>', $pub_der) or die "failed to open pub der\n";
binmode($pub_fh) or die "failed to set binary mode on pub der\n";
print {$pub_fh} $pubout or die "failed to write pub der\n";
close($pub_fh) or die "failed to close pub der\n";

run_cmd('openssl', 'dgst', '-sha256', '-binary', '-out', $digest, "$pkg_root/manifest.toml");
open(my $msg_fh, '>', $message) or die "failed to open message\n";
binmode($msg_fh) or die "failed to set binary mode on message\n";
print {$msg_fh} "mochios-mpkg-manifest-v1\0" or die "failed to write prefix\n";
open(my $digest_fh, '<', $digest) or die "failed to open digest\n";
binmode($digest_fh) or die "failed to set binary mode on digest\n";
local $/;
my $digest_bytes = <$digest_fh>;
close($digest_fh) or die "failed to close digest\n";
print {$msg_fh} $digest_bytes or die "failed to write digest\n";
close($msg_fh) or die "failed to close message\n";

run_cmd(
    'openssl', 'pkeyutl',
    '-sign',
    '-rawin',
    '-inkey', $private_key,
    '-in', $message,
    '-out', $sig,
);

open(my $sig_fh, '<', $sig) or die "failed to open signature\n";
binmode($sig_fh) or die "failed to set binary mode on signature\n";
local $/;
my $sig_bytes = <$sig_fh>;
close($sig_fh) or die "failed to close signature\n";
length($sig_bytes) == 64 or die "unexpected signature length\n";

open(my $cert_fh, '>', "$signatures_root/developer.cert") or die "failed to open cert\n";
binmode($cert_fh) or die "failed to set binary mode on cert\n";
print {$cert_fh} substr($pubout, -32) or die "failed to write cert\n";
close($cert_fh) or die "failed to close cert\n";

open(my $manifest_sig_fh, '>', "$signatures_root/manifest.sig") or die "failed to open manifest sig\n";
binmode($manifest_sig_fh) or die "failed to set binary mode on manifest sig\n";
print {$manifest_sig_fh} $sig_bytes or die "failed to write manifest sig\n";
close($manifest_sig_fh) or die "failed to close manifest sig\n";

my $output_dir = dirname($output);
if (defined $output_dir && length($output_dir) && !-d $output_dir) {
    make_path($output_dir) or die "failed to create output dir: $!\n";
}

my $payload_rel = 'payload/root' . $binary_path;
run_cmd(
    'tar',
    '--format=ustar',
    '--owner=0',
    '--group=0',
    '--numeric-owner',
    '-C',
    $pkg_root,
    '-cf',
    $tarfile,
    'manifest.toml',
    'signatures/manifest.sig',
    'signatures/developer.cert',
    $payload_rel,
);

open(my $tar_fh, '<', $tarfile) or die "failed to open tar\n";
binmode($tar_fh) or die "failed to set binary mode on tar\n";
local $/;
my $tar_bytes = <$tar_fh>;
close($tar_fh) or die "failed to close tar\n";

my $header = pack('A4 v v v C C Q< a12', 'MPKG', 1, 0, 32, 0, 0, length($tar_bytes), "\0" x 12);

open(my $out_fh, '>', $output) or die "failed to open output: $!\n";
binmode($out_fh) or die "failed to set binary mode on output: $!\n";
print {$out_fh} $header or die "failed to write header\n";
print {$out_fh} $tar_bytes or die "failed to write tar payload\n";
close($out_fh) or die "failed to close output: $!\n";
