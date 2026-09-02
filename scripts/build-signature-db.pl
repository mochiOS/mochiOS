#!/usr/bin/env perl

use strict;
use warnings;

use Digest::SHA qw(sha256_hex);
use File::Basename qw(dirname);
use File::Path qw(make_path);

sub die_usage {
    die "usage: $0 --output PATH --entry NAME=FILE [--entry NAME=FILE ...]\n";
}

my $output;
my @entries;
while (@ARGV) {
    my $arg = shift @ARGV;
    if ($arg eq '--output') {
        @ARGV or die_usage();
        $output = shift @ARGV;
    }
    elsif ($arg eq '--entry') {
        @ARGV or die_usage();
        push @entries, shift @ARGV;
    }
    else {
        die_usage();
    }
}

defined $output or die_usage();
@entries or die_usage();

my $output_dir = dirname($output);
if (defined $output_dir && length($output_dir) && !-d $output_dir) {
    make_path($output_dir) or die "failed to create $output_dir: $!\n";
}

open(my $out_fh, '>', $output) or die "failed to open $output: $!\n";
binmode($out_fh) or die "failed to set binary mode on $output: $!\n";
print {$out_fh} "mnu-execution-allowlist v1\n";

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
    print {$out_fh} 'record ', $path, ' ', sha256_hex($bytes), "\n";
}

close($out_fh) or die "failed to close $output: $!\n";
