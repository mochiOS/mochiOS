#!/usr/bin/env perl

use strict;
use warnings;
use utf8;
use File::Basename qw(dirname basename);
use File::Find qw(find);
use XML::Parser;

binmode STDIN, ':encoding(UTF-8)';
binmode STDOUT, ':encoding(UTF-8)';
binmode STDERR, ':encoding(UTF-8)';

if (!-d '.repo') {
    die "error: repoワークスペースのルートで実行してください\n";
}

my %manifest_projects = manifest_projects('default.xml');
my @repositories = git_repositories('.');
my %repository_paths = map { $_ => 1 } @repositories;

my @targets;
my $preflight_failed = 0;
for my $path (sort keys %manifest_projects) {
    next if $repository_paths{$path};
    print STDERR "[error] $path: manifestに登録されていますがGitリポジトリがありません\n";
    $preflight_failed = 1;
}

for my $path (@repositories) {
    my $manifest = $manifest_projects{$path};
    my ($project, $remote) = github_mochios_remote($path, $manifest);
    if (!defined $project) {
        if (defined $manifest) {
            print STDERR "[error] $path: $manifest->{project} へpushできるremoteがありません\n";
            $preflight_failed = 1;
        }
        next;
    }

    my $revision = defined $manifest
        ? $manifest->{revision}
        : remote_default_branch($path, $remote);
    $revision //= '';
    $revision =~ s{^refs/heads/}{};
    if ($revision eq '') {
        print STDERR "[error] $path: push先branchを特定できません\n";
        $preflight_failed = 1;
        next;
    }

    unless (git_fetch_branch($path, $remote, $revision)) {
        print STDERR "[error] $path: $remote/$revision の取得に失敗しました\n";
        $preflight_failed = 1;
        next;
    }

    my ($ahead, $behind) = git_ahead_behind($path, $remote, $revision);
    unless (defined $ahead && defined $behind) {
        print STDERR "[error] $path: $remote/$revision との差分を確認できません\n";
        $preflight_failed = 1;
        next;
    }

    if ($behind > 0) {
        my $state = $ahead > 0 ? '分岐しています' : 'pullが必要です';
        print STDERR "[pull required] $path: $remote/$revision より $behind commit遅れており、$state\n";
        $preflight_failed = 1;
        next;
    }

    next unless $ahead > 0;
    push @targets, {
        project  => $project,
        path     => $path,
        remote   => $remote,
        revision => $revision,
    };
}

if ($preflight_failed) {
    die "error: remote側の変更を取り込んでから再実行してください\n";
}

if (!@targets) {
    print "push対象のリポジトリはありません\n";
    exit 0;
}

print "push対象のリポジトリ:\n";
for my $target (@targets) {
    print "  $target->{project} ($target->{path}) -> $target->{remote}/$target->{revision}\n";
}

while (1) {
    print "\n上記", scalar(@targets), "リポジトリをpushしますか？ [Y/n]: ";

    my $answer = <STDIN>;
    if (!defined $answer) {
        print "\n入力が終了したため中断します\n";
        exit 1;
    }

    chomp $answer;
    $answer =~ s/^\s+|\s+$//g;
    $answer = lc $answer;

    if ($answer eq 'n' || $answer eq 'no') {
        print "[cancel] pushを中断しました\n";
        exit 0;
    }
    last if $answer eq '' || $answer eq 'y' || $answer eq 'yes';
    print "y、n、または空欄で入力してください\n";
}

for my $target (@targets) {
    print "[push] $target->{path} -> $target->{remote}/$target->{revision}\n";

    my $result = system(
        'git',
        '-C', $target->{path},
        'push',
        $target->{remote},
        "HEAD:refs/heads/$target->{revision}",
    );

    if ($result != 0) {
        print STDERR "[error] $target->{path} のpushに失敗しました\n";
        exit 1;
    }
}

print "\n[done] 処理が完了しました\n";

sub manifest_projects {
    my ($manifest_path) = @_;
    my %projects;
    my $parser = XML::Parser->new(
        Handlers => {
            Start => sub {
                my ($expat, $element, %attributes) = @_;
                return unless $element eq 'project';
                return unless defined $attributes{name};
                return unless $attributes{name} =~ m{^mochiOS/};
                my $path = $attributes{path} // $attributes{name};
                $projects{$path} = {
                    project  => $attributes{name},
                    revision => $attributes{revision} // '',
                };
            },
        },
    );
    eval { $parser->parsefile($manifest_path) };
    die "error: $manifest_path を解析できません: $@" if $@;
    return %projects;
}

sub git_repositories {
    my ($root) = @_;
    my @repositories;
    find(
        {
            no_chdir => 1,
            wanted   => sub {
                my $path = $File::Find::name;
                my $name = basename($path);
                if (-d $path && ($name eq '.repo' || $name eq 'out')) {
                    $File::Find::prune = 1;
                    return;
                }
                return unless $name eq '.git';
                my $repository = dirname($path);
                $repository =~ s{^\./}{};
                $repository = '.' if $repository eq '';
                push @repositories, $repository;
                $File::Find::prune = 1 if -d $path;
            },
        },
        $root,
    );
    my %seen;
    return sort {
        ($a ne '.') <=> ($b ne '.') || $a cmp $b
    } grep { !$seen{$_}++ } @repositories;
}

sub github_mochios_remote {
    my ($path, $manifest) = @_;
    my $remotes = git_output('git', '-C', $path, 'remote');
    return unless defined $remotes;
    my @matches;
    for my $remote (split /\n/, $remotes) {
        my $url = git_output('git', '-C', $path, 'remote', 'get-url', '--push', $remote);
        next unless defined $url;
        next unless $url =~ m{github\.com(?::|/)(mochiOS/[^/]+?)(?:\.git)?/?$}i;
        push @matches, [$1, $remote];
    }
    if (defined $manifest) {
        for my $match (@matches) {
            return @$match if lc($match->[0]) eq lc($manifest->{project});
        }
        return;
    }
    return unless @matches;
    @matches = sort {
        ($a->[1] ne 'origin') <=> ($b->[1] ne 'origin')
            || $a->[1] cmp $b->[1]
    } @matches;
    return @{$matches[0]};
}

sub remote_default_branch {
    my ($path, $remote) = @_;
    my $symbolic = git_output(
        'git', '-C', $path,
        'symbolic-ref', '--short', "refs/remotes/$remote/HEAD",
    );
    return unless defined $symbolic;
    $symbolic =~ s{^\Q$remote\E/}{};
    return $symbolic;
}

sub git_fetch_branch {
    my ($path, $remote, $revision) = @_;
    my $refspec = "+refs/heads/$revision:refs/remotes/$remote/$revision";
    return system(
        'git', '-C', $path,
        'fetch', '--quiet', $remote, $refspec,
    ) == 0;
}

sub git_ahead_behind {
    my ($path, $remote, $revision) = @_;
    my $counts = git_output(
        'git', '-C', $path,
        'rev-list', '--left-right', '--count',
        "HEAD...refs/remotes/$remote/$revision",
    );
    return unless defined $counts;
    return unless $counts =~ /^([0-9]+)\s+([0-9]+)$/;
    return ($1 + 0, $2 + 0);
}

sub git_output {
    my (@command) = @_;
    open my $fh, '-|', @command or return undef;
    local $/;
    my $output = <$fh>;
    close $fh or return undef;
    $output //= '';
    $output =~ s/\s+\z//;
    return $output;
}
