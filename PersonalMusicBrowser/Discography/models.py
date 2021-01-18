from django.db import models

import os


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


class Discography(models.Model):
    """
    This represents the root storage element of music.
    For me this is OneDrive. So the storage root path is OneDrive's mount point. And the type is the string 'onedrive'
    """
    storage_root_path = models.FilePathField(path=os.path.expanduser('~') or '.', blank=False, allow_files=False, allow_folders=True)
    type = models.TextField(blank=False, choices=[("Onedrive",)*2, ("Directory",)*2])


class Song(models.Model):
    title = models.TextField(max_length=256, blank=False)
    sheet_music = models.FilePathField(path="", blank=True)
    lyrics = models.FilePathField(path=".", blank=True)
    album = models.ForeignKey(Album, on_delete=models.PROTECT)
    artists = models.ManyToManyField(Artist)
    discography = models.ForeignKey(Discography, on_delete=models.PROTECT)


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
    path = models.FilePathField(path=".", blank=True)
    song = models.ForeignKey(Song, blank=False, on_delete=models.PROTECT)
    notes = models.ImageField(blank=True)
