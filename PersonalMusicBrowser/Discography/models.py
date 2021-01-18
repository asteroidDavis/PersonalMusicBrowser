from django.conf import settings
from django.db import models

import pathlib


class Instrument(models.Model):
    name = models.TextField(max_length="64", blank=False, primary_key=True)


class Band(models.Model):
    name = models.TextField(max_length=128, blank=False)


class Artist(models.Model):
    name = models.TextField(max_length=128, blank=False)
    bands = models.ManyToManyField(Band)


class Album(models.Model):
    title = models.TextField(max_length=256, primary_key=True, blank=False)
    released = models.BooleanField(blank=False)
    url = models.URLField(blank=True)


def get_discography_path() -> str:
    return settings.DISCOGRAPHY_ROOT


class Song(models.Model):
    title = models.TextField(max_length=256, blank=False)
    album = models.ForeignKey(Album, on_delete=models.PROTECT)
    artists = models.ManyToManyField(Artist)
    sheet_music = models.FilePathField(path=get_discography_path(), blank=True, allow_files=True, allow_folders=False,
                                       recursive=True)
    lyrics = models.FilePathField(path=get_discography_path(), blank=True, allow_files=True, allow_folders=False,
                                  recursive=True)


class Cover(Song):
    notes = models.ImageField(blank=True)
    covered_instruments = models.ManyToManyField(Instrument)
    notes_completed = models.BooleanField()


class Composition(Song):
    written_instruments = models.ManyToManyField(Instrument)
    beats_per_minute_upper = models.IntegerField()
    beats_per_minute_lower = models.IntegerField()


class Recording(models.Model):
    instruments = models.ManyToManyField(Instrument)
    type = models.TextField(max_length=64, choices=[
        ('audacity',)*2, ('mix',)*2, ('master',)*2, ('loop-core-list',)*2, ('wav',)*2, ('audacity',)*2])
    path = models.FilePathField(path=get_discography_path(), blank=True, allow_files=True, allow_folders=False,
                                recursive=True)
    song = models.ForeignKey(Song, blank=False, on_delete=models.PROTECT)
    notes = models.ImageField(blank=True)
