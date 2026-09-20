#!/usr/bin/env perl
use strict;
use warnings;
use Getopt::Long qw(GetOptions);

my ($manifest, $elf, $out);
my @deps;

GetOptions(
    'manifest=s' => \$manifest,
    'elf=s'     => \$elf,
    'out=s'     => \$out,
    'dep=s'     => \@deps,
) or die "usage: $0 --manifest manifest.toml --elf MODULE.elf --out entry [--dep disk]\n";

defined $manifest && defined $elf && defined $out
    or die "missing required arguments\n";

open my $manifest_fh, '<', $manifest or die "open $manifest: $!";
my ($name, $version, $abi);
my $section = '';
while (my $line = <$manifest_fh>) {
    $line =~ s/#.*$//;
    if ($line =~ /^\s*\[([^]]+)\]\s*$/) {
        $section = $1;
        next;
    }
    next unless $section eq 'cext';
    $name = $1 if $line =~ /^\s*name\s*=\s*"([^"]+)"\s*$/;
    $version = $1 if $line =~ /^\s*version\s*=\s*(\d+)\s*$/;
    $abi = $1 if $line =~ /^\s*abi\s*=\s*(\d+)\s*$/;
}
close $manifest_fh;
defined $name && defined $version && defined $abi && $abi > 0 && $abi <= 65535
    or die "invalid cext name, version, or ABI in $manifest\n";

open my $elf_fh, '<:raw', $elf or die "open $elf: $!";
local $/ = undef;
my $elf_bytes = <$elf_fh>;
close $elf_fh;

my $name_len = length($name);
my $dep_count = scalar(@deps);
my $header_size = 32 + $name_len;
for my $dep (@deps) {
    $header_size += 2 + length($dep);
}

open my $out_fh, '>:raw', $out or die "open $out: $!";
print {$out_fh} "MCEX";
print {$out_fh} pack('v', $abi);
print {$out_fh} pack('v', $version);
print {$out_fh} pack('v', $name_len);
print {$out_fh} pack('v', $dep_count);
print {$out_fh} pack('V', $header_size);
print {$out_fh} pack('Q<', length($elf_bytes));
print {$out_fh} pack('Q<', 0);
print {$out_fh} $name;
for my $dep (@deps) {
    print {$out_fh} pack('v', length($dep));
    print {$out_fh} $dep;
}
print {$out_fh} $elf_bytes;
close $out_fh;
