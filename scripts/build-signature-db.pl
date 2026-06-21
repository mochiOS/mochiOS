#!/usr/bin/env perl

use strict;
use warnings;

use Digest::SHA qw(sha256);
use File::Basename qw(dirname);
use File::Path qw(make_path);
use File::Temp qw(tempdir);

my $DOMAIN = "mnu-signature-v1\0";

sub die_usage {
    die "usage: $0 --output PATH --entry NAME=FILE [--entry NAME=FILE ...]\n";
}

sub hex_encode {
    return unpack('H*', $_[0]);
}

sub run_cmd_capture {
    my (@cmd) = @_;
    open(my $fh, '-|', @cmd) or die "failed to run @cmd: $!\n";
    local $/;
    my $data = <$fh>;
    close($fh) or die "command failed: @cmd\n";
    return $data;
}

sub run_cmd {
    my (@cmd) = @_;
    system(@cmd) == 0 or die "command failed: @cmd\n";
}

my $output;
my @entries;
while (@ARGV) {
    my $arg = shift @ARGV;
    if ($arg eq '--output') {
        @ARGV or die_usage();
        $output = shift @ARGV;
    } elsif ($arg eq '--entry') {
        @ARGV or die_usage();
        push @entries, shift @ARGV;
    } else {
        die_usage();
    }
}

defined $output or die_usage();
@entries or die_usage();

my $tmpdir = tempdir(CLEANUP => 1);
my $private_key = "$tmpdir/ed25519.key";
my $message = "$tmpdir/message.bin";
my $signature = "$tmpdir/signature.bin";

run_cmd('openssl', 'genpkey', '-algorithm', 'ed25519', '-out', $private_key);

my $pub_der = run_cmd_capture('openssl', 'pkey', '-in', $private_key, '-pubout', '-outform', 'DER');
length($pub_der) >= 32 or die "public key output too short\n";
my $pubkey = substr($pub_der, -32);
length($pubkey) == 32 or die "failed to extract raw public key\n";

my $output_dir = dirname($output);
if (defined $output_dir && length($output_dir) && !-d $output_dir) {
    make_path($output_dir) or die "failed to create $output_dir: $!\n";
}

open(my $out_fh, '>', $output) or die "failed to open $output: $!\n";
binmode($out_fh) or die "failed to set binary mode on $output: $!\n";

print {$out_fh} "mnu-signature-db v1\n";
print {$out_fh} 'pubkey ', hex_encode($pubkey), "\n";

for my $entry (@entries) {
    my ($path, $file) = split /=/, $entry, 2;
    defined $path && defined $file && length($path) && length($file)
        or die "bad entry: $entry\n";
    -f $file or die "missing entry file: $file\n";

    open(my $in_fh, '<', $file) or die "failed to open $file: $!\n";
    binmode($in_fh) or die "failed to set binary mode on $file: $!\n";
    local $/;
    my $bytes = <$in_fh>;
    close($in_fh) or die "failed to close $file: $!\n";

    my $digest = sha256($bytes);
    my $payload = $DOMAIN . $path . "\0" . $digest;

    open(my $msg_fh, '>', $message) or die "failed to open $message: $!\n";
    binmode($msg_fh) or die "failed to set binary mode on $message: $!\n";
    print {$msg_fh} $payload or die "failed to write message payload\n";
    close($msg_fh) or die "failed to close $message: $!\n";

    run_cmd(
        'openssl', 'pkeyutl',
        '-sign',
        '-rawin',
        '-inkey', $private_key,
        '-in', $message,
        '-out', $signature,
    );

    open(my $sig_fh, '<', $signature) or die "failed to open $signature: $!\n";
    binmode($sig_fh) or die "failed to set binary mode on $signature: $!\n";
    local $/;
    my $sig_bytes = <$sig_fh>;
    close($sig_fh) or die "failed to close $signature: $!\n";
    length($sig_bytes) == 64 or die "unexpected signature length for $path\n";

    print {$out_fh} 'record ', $path, ' ', hex_encode($digest), ' ', hex_encode($sig_bytes), "\n";
}

close($out_fh) or die "failed to close $output: $!\n";
