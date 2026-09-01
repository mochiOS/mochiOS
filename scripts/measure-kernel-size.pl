#!/usr/bin/env perl

use strict;
use warnings;

use Cwd qw(abs_path);
use File::Basename qw(dirname);
use File::Spec;
use FindBin;
use Getopt::Long qw(GetOptions);
use JSON::PP;
use POSIX qw(strftime);

my $workspace = abs_path("$FindBin::Bin/..");
my $kernel = "$workspace/out/artifacts/kernel.elf";
my $output;
my $profile = 'release';
my @features = ('kernel-bin');

GetOptions(
    'kernel=s'   => \$kernel,
    'output=s'   => \$output,
    'profile=s'  => \$profile,
    'feature=s@' => \@features,
) or die "usage: $0 [--kernel PATH] [--output PATH] [--profile NAME] [--feature NAME]\n";

$kernel = abs_path($kernel) // die "kernel was not found: $kernel\n";
die "kernel is not a regular file: $kernel\n" if !-f $kernel;

my $readelf = $ENV{READELF} // 'readelf';
my $nm = $ENV{NM} // 'nm';

sub capture {
    my (@command) = @_;
    open my $handle, '-|', @command or die "failed to run $command[0]: $!\n";
    local $/;
    my $text = <$handle> // '';
    close $handle or die "command failed: @command\n";
    return $text;
}

sub first_line {
    my ($text) = @_;
    $text =~ s/\r//g;
    return ($text =~ /^([^\n]*)/) ? $1 : '';
}

sub git_revision {
    my ($repository) = @_;
    return first_line(capture('git', '-C', $repository, 'rev-parse', 'HEAD'));
}

sub parse_sections {
    my ($text) = @_;
    my @sections;

    for my $line (split /\n/, $text) {
        next if $line !~ /^\s*\[\s*(\d+)\]\s+(\S+)\s+(\S+)\s+([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+(\S*)\s+(\d+)\s+(\d+)\s+(\d+)\s*$/;
        push @sections, {
            index         => 0 + $1,
            name          => $2,
            type          => $3,
            address       => hex($4),
            file_offset   => hex($5),
            size_bytes    => hex($6),
            entry_bytes   => hex($7),
            flags         => $8,
            link          => 0 + $9,
            info          => 0 + $10,
            alignment     => 0 + $11,
        };
    }

    die "no ELF sections were parsed\n" if !@sections;
    return \@sections;
}

sub parse_segments {
    my ($text) = @_;
    my @segments;

    for my $line (split /\n/, $text) {
        next if $line !~ /^\s*LOAD\s+(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+(.+?)\s+(0x[0-9a-fA-F]+)\s*$/;
        push @segments, {
            file_offset => hex($1),
            virtual_address => hex($2),
            physical_address => hex($3),
            file_size_bytes => hex($4),
            memory_size_bytes => hex($5),
            flags => $6,
            alignment_bytes => hex($7),
        };
    }

    die "no loadable ELF segments were parsed\n" if !@segments;
    return \@segments;
}

sub parse_symbols {
    my ($text) = @_;
    my @symbols;

    for my $line (split /\n/, $text) {
        next if $line !~ /^\s*(\d+)\s+(\d+)\s+(\S)\s+(.+)$/;
        push @symbols, {
            address => 0 + $1,
            size_bytes => 0 + $2,
            type => $3,
            name => $4,
        };
    }

    return \@symbols;
}

sub sum_section_sizes {
    my ($sections, $predicate) = @_;
    my $total = 0;
    $total += $_->{size_bytes} for grep { $predicate->($_) } @{$sections};
    return $total;
}

sub section_size {
    my ($sections, $name) = @_;
    for my $section (@{$sections}) {
        return $section->{size_bytes} if $section->{name} eq $name;
    }
    return 0;
}

sub symbol_storage {
    my ($type) = @_;
    my %storage = (
        b => '.bss',
        d => '.data',
        r => '.rodata',
        t => '.text',
    );
    return $storage{lc $type} // 'other';
}

sub selected_symbols {
    my ($symbols) = @_;
    my @patterns = qw(
        MAILBOXES
        KSTACK_POOL
        RING0_STACKS
        IST_STACKS
        AP_BOOT_STACKS
        KERNEL_THREAD_STACK
    );
    my @selected;

    for my $symbol (@{$symbols}) {
        next if !grep { index($symbol->{name}, $_) >= 0 } @patterns;
        push @selected, {
            name => $symbol->{name},
            size_bytes => $symbol->{size_bytes},
            storage => symbol_storage($symbol->{type}),
        };
    }

    return [sort { $b->{size_bytes} <=> $a->{size_bytes} } @selected];
}

sub host_metadata {
    my $cpu = '';
    if (open my $cpuinfo, '<', '/proc/cpuinfo') {
        while (my $line = <$cpuinfo>) {
            if ($line =~ /^model name\s*:\s*(.+?)\s*$/) {
                $cpu = $1;
                last;
            }
        }
        close $cpuinfo;
    }

    my $memory_bytes = 0;
    if (open my $meminfo, '<', '/proc/meminfo') {
        while (my $line = <$meminfo>) {
            if ($line =~ /^MemTotal:\s+(\d+)\s+kB/) {
                $memory_bytes = $1 * 1024;
                last;
            }
        }
        close $meminfo;
    }

    return {
        cpu => $cpu,
        cpu_count => 0 + first_line(capture('nproc')),
        memory_bytes => $memory_bytes,
        environment => 'build-host',
    };
}

my $sections = parse_sections(capture($readelf, '--wide', '--section-headers', $kernel));
my $segments = parse_segments(capture($readelf, '--wide', '--program-headers', $kernel));
my $symbols = parse_symbols(capture($nm, '-S', '--size-sort', '--radix=d', $kernel));

my $load_file_bytes = 0;
my $load_memory_bytes = 0;
my $max_segment_alignment = 0;
for my $segment (@{$segments}) {
    $load_file_bytes += $segment->{file_size_bytes};
    $load_memory_bytes += $segment->{memory_size_bytes};
    $max_segment_alignment = $segment->{alignment_bytes}
        if $segment->{alignment_bytes} > $max_segment_alignment;
}

my $inter_segment_padding = 0;
my @ordered_segments = sort { $a->{file_offset} <=> $b->{file_offset} } @{$segments};
for my $index (1 .. $#ordered_segments) {
    my $previous_end = $ordered_segments[$index - 1]->{file_offset}
        + $ordered_segments[$index - 1]->{file_size_bytes};
    my $gap = $ordered_segments[$index]->{file_offset} - $previous_end;
    $inter_segment_padding += $gap if $gap > 0;
}

my $tracked_symbols = selected_symbols($symbols);
my ($mailboxes) = grep { index($_->{name}, 'MAILBOXES') >= 0 } @{$tracked_symbols};

my $toolchain = 'nightly-2026-05-14';
if (open my $config, '<', "$workspace/build/config.mk") {
    while (my $line = <$config>) {
        if ($line =~ /^KERNEL_RUST_TOOLCHAIN\s*:=\s*(\S+)/) {
            $toolchain = $1;
            last;
        }
    }
    close $config;
}

my $file_size = -s $kernel;
my $report = {
    schema_version => 1,
    generated_at_utc => strftime('%Y-%m-%dT%H:%M:%SZ', gmtime),
    artifact => File::Spec->abs2rel($kernel, $workspace),
    revisions => {
        workspace => git_revision($workspace),
        mnu => git_revision("$workspace/core"),
    },
    build => {
        profile => $profile,
        features => [sort @features],
        rust_toolchain => $toolchain,
        rustc => first_line(capture('rustc', "+$toolchain", '--version', '--verbose')),
        linker => first_line(capture('ld', '--version')),
    },
    host => host_metadata(),
    measurement => {
        iterations => 1,
        cache_state => 'not-applicable',
        file_size_bytes => $file_size,
        loadable_file_size_bytes => $load_file_bytes,
        loadable_memory_size_bytes => $load_memory_bytes,
        non_loadable_file_bytes => $file_size - $load_file_bytes,
        inter_segment_padding_bytes => $inter_segment_padding,
        max_segment_alignment_bytes => $max_segment_alignment,
        sections => $sections,
        section_totals => {
            text_bytes => section_size($sections, '.text'),
            rodata_bytes => section_size($sections, '.rodata'),
            data_bytes => section_size($sections, '.data'),
            bss_bytes => section_size($sections, '.bss'),
            relocation_bytes => sum_section_sizes(
                $sections,
                sub { $_[0]->{name} =~ /^\.rela?(?:\.|$)/ },
            ),
            symbol_table_bytes => sum_section_sizes(
                $sections,
                sub { $_[0]->{name} =~ /^\.(?:symtab|strtab|shstrtab)$/ },
            ),
            debug_bytes => sum_section_sizes(
                $sections,
                sub { $_[0]->{name} =~ /^\.debug(?:_|\.)/ },
            ),
        },
        loadable_segments => $segments,
        fixed_regions => $tracked_symbols,
        mailboxes => {
            size_bytes => $mailboxes ? $mailboxes->{size_bytes} : 0,
            storage => $mailboxes ? $mailboxes->{storage} : 'missing',
            initialized_data_reason =>
                'Mailbox::new stores the free-slot indices and a non-zero free_count, so the current representation is not all-zero initializable.',
        },
    },
};

my $json = JSON::PP->new->canonical->pretty->encode($report);
if (defined $output) {
    open my $handle, '>', $output or die "failed to open $output: $!\n";
    print {$handle} $json or die "failed to write $output: $!\n";
    close $handle or die "failed to close $output: $!\n";
} else {
    print $json;
}
